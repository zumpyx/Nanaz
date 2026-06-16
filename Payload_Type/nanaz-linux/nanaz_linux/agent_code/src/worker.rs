#[cfg(windows)]
use mythic::{TaskMessage, TaskResponse};

#[cfg(windows)]
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::io::{Read, Write};
#[cfg(windows)]
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
const WORKER_ENV: &str = "NANAZ_TASK_WORKER";
#[cfg(windows)]
const POLL_DELAY: Duration = Duration::from_millis(50);
#[cfg(windows)]
const MAX_WORKER_OUTPUT: usize = 8 * 1024 * 1024;

#[cfg(windows)]
#[derive(Serialize, Deserialize)]
struct WorkerRequest {
    task_id: uuid::Uuid,
    command: String,
    parameters: String,
}

#[cfg(windows)]
pub fn in_worker() -> bool {
    std::env::var_os(WORKER_ENV).is_some()
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn in_worker() -> bool {
    false
}

#[cfg(windows)]
pub fn run_from_stdin() -> i32 {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        let _ = writeln!(std::io::stderr(), "worker stdin read failed: {e}");
        return 2;
    }
    let request = match serde_json::from_str::<WorkerRequest>(&input) {
        Ok(request) => request,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "worker request parse failed: {e}");
            return 2;
        }
    };
    let task = TaskMessage {
        id: request.task_id,
        command: request.command,
        parameters: request.parameters,
        ..Default::default()
    };
    let response = crate::dispatch::dispatch(&task);
    match serde_json::to_writer(std::io::stdout(), &response) {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "worker response serialize failed: {e}");
            2
        }
    }
}

#[cfg(not(windows))]
pub fn run_from_stdin() -> i32 {
    2
}

#[cfg(windows)]
pub fn run_isolated_task(task: &TaskMessage, timeout_secs: u64) -> TaskResponse {
    if in_worker() {
        return TaskResponse::failed(task.id, "nested task worker execution refused");
    }
    if timeout_secs == 0 || timeout_secs > 3600 {
        return TaskResponse::failed(task.id, "worker timeout must be between 1 and 3600 seconds");
    }

    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            return TaskResponse::failed(task.id, &format!("resolve worker path failed: {e}"));
        }
    };
    let request = WorkerRequest {
        task_id: task.id,
        command: task.command.clone(),
        parameters: task.parameters.clone(),
    };
    let request_json = match serde_json::to_vec(&request) {
        Ok(json) => json,
        Err(e) => {
            return TaskResponse::failed(task.id, &format!("serialize worker request failed: {e}"));
        }
    };

    let mut child = match Command::new(exe)
        .arg("--nanaz-task-worker")
        .env(WORKER_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return TaskResponse::failed(task.id, &format!("spawn task worker failed: {e}")),
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(&request_json) {
            let _ = child.kill();
            let _ = child.wait();
            return TaskResponse::failed(task.id, &format!("write worker request failed: {e}"));
        }
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_thread = stdout.map(|stdout| std::thread::spawn(move || read_limited(stdout)));
    let stderr_thread = stderr.map(|stderr| std::thread::spawn(move || read_limited(stderr)));

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return TaskResponse::failed(
                        task.id,
                        &format!("task worker timed out after {timeout_secs}s"),
                    );
                }
                std::thread::sleep(POLL_DELAY);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return TaskResponse::failed(task.id, &format!("poll task worker failed: {e}"));
            }
        }
    };

    let (stdout, stdout_truncated) = stdout_thread
        .and_then(|thread| thread.join().ok())
        .unwrap_or_default();
    let (stderr, stderr_truncated) = stderr_thread
        .and_then(|thread| thread.join().ok())
        .unwrap_or_default();
    if !status.success() {
        return TaskResponse::failed(
            task.id,
            &format!(
                "task worker exited with {status}; stderr: {}",
                String::from_utf8_lossy(&stderr)
            ),
        );
    }
    match serde_json::from_slice::<TaskResponse>(&stdout) {
        Ok(mut response) => {
            if stdout_truncated || stderr_truncated {
                let note = "\n[worker output truncated]";
                response.user_output = Some(format!(
                    "{}{}",
                    response.user_output.unwrap_or_default(),
                    note
                ));
            }
            response
        }
        Err(e) => TaskResponse::failed(
            task.id,
            &format!(
                "parse task worker response failed: {e}; stdout: {}; stderr: {}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            ),
        ),
    }
}

#[cfg(windows)]
fn read_limited<R: Read>(reader: R) -> (Vec<u8>, bool) {
    let mut limited = reader.take((MAX_WORKER_OUTPUT + 1) as u64);
    let mut buf = Vec::new();
    let _ = limited.read_to_end(&mut buf);
    let truncated = buf.len() > MAX_WORKER_OUTPUT;
    if truncated {
        buf.truncate(MAX_WORKER_OUTPUT);
    }
    (buf, truncated)
}
