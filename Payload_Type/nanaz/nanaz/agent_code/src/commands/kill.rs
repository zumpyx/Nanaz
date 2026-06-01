//! Terminate a process by PID — cross-platform.
//!
//! Linux/macOS: use the libc `kill(pid, SIGTERM)` syscall. If SIGTERM
//! does not have permission (e.g. process owned by another user), try
//! `SIGKILL` once before reporting failure — many poorly-written
//! services don't catch SIGTERM anyway, so this gives the operator a
//! useful "you asked nicely, then forcefully" path without an extra
//! Mythic round-trip.
//!
//! Windows: open the process with `OpenProcess` and call
//! `TerminateProcess`. Exit code 1 is used for both graceful and
//! forced termination — there is no clean way to distinguish them
//! once the process is gone.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "pid": 1234,
//!     "force": false       // optional, default false; on Unix escalate to SIGKILL
//! }
//! ```
//!
//! Response: `TaskResponse.user_output` describes the outcome so the
//! operator sees "killed 1234" vs "permission denied" without a separate
//! error log.

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize)]
struct Params {
    pid: i64,
    #[serde(default)]
    force: bool,
}

// ── Linux/macOS ─────────────────────────────────────────────

#[cfg(unix)]
pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("kill parse error: {e}")),
    };

    if params.pid <= 0 {
        return TaskResponse::failed(
            task.id,
            &format!("invalid pid {} (must be > 0)", params.pid),
        );
    }

    // First attempt: SIGTERM. This is the polite way — well-behaved
    // daemons shut down cleanly, flush logs, and release locks.
    let term_result = unsafe { libc::kill(params.pid as i32, libc::SIGTERM) };
    if term_result == 0 {
        return TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!("sent SIGTERM to pid {}", params.pid)),
            ..Default::default()
        };
    }

    // If SIGTERM failed because the process is already gone, treat
    // that as success — the operator's goal ("no more process") is met.
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        return TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!("pid {} already exited", params.pid)),
            ..Default::default()
        };
    }
    if !params.force {
        return TaskResponse::failed(
            task.id,
            &format!(
                "SIGTERM to {} failed: {}; pass force=true to try SIGKILL",
                params.pid, err
            ),
        );
    }

    // Escalate to SIGKILL — uncatchable, immediate.
    let kill_result = unsafe { libc::kill(params.pid as i32, libc::SIGKILL) };
    if kill_result == 0 {
        return TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!("sent SIGKILL to pid {}", params.pid)),
            ..Default::default()
        };
    }

    let err2 = std::io::Error::last_os_error();
    TaskResponse::failed(
        task.id,
        &format!(
            "kill {} failed (SIGTERM: {}, SIGKILL: {})",
            params.pid, err, err2
        ),
    )
}

// ── Windows ────────────────────────────────────────────────

#[cfg(windows)]
pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("kill parse error: {e}")),
    };

    if params.pid <= 0 {
        return TaskResponse::failed(
            task.id,
            &format!("invalid pid {} (must be > 0)", params.pid),
        );
    }

    // PROCESS_TERMINATE (0x0001) is the minimum privilege needed.
    // We don't request PROCESS_QUERY_INFORMATION or any of the
    // higher rights — open with the lowest privilege that lets us
    // do the job, and let Windows return ERROR_ACCESS_DENIED if
    // we don't have it.
    //
    // SAFETY: pure FFI; all inputs are POD.
    let handle = unsafe {
        windows_sys::Win32::System::Threading::OpenProcess(
            windows_sys::Win32::System::Threading::PROCESS_TERMINATE,
            0,
            params.pid as u32,
        )
    };
    if handle == 0 {
        let err = std::io::Error::last_os_error();
        return TaskResponse::failed(
            task.id,
            &format!("OpenProcess({}) failed: {}", params.pid, err),
        );
    }

    // Exit code 1 is a common "killed by signal" sentinel. The exact
    // value is mostly cosmetic — the process is going away — but using
    // a non-zero code helps post-mortem tools (WER, etcd, journalctl)
    // distinguish "exited cleanly" from "killed".
    //
    // SAFETY: handle is a valid open process; exit code is a DWORD.
    let term_result = unsafe {
        windows_sys::Win32::System::Threading::TerminateProcess(handle, 1)
    };
    // Always close the handle — leaking it would prevent the OS from
    // fully reaping the process object.
    unsafe {
        windows_sys::Win32::Foundation::CloseHandle(handle);
    }
    if term_result == 0 {
        let err = std::io::Error::last_os_error();
        return TaskResponse::failed(
            task.id,
            &format!("TerminateProcess({}) failed: {}", params.pid, err),
        );
    }

    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(format!("terminated pid {}", params.pid)),
        ..Default::default()
    }
}

#[cfg(not(any(unix, windows)))]
pub fn handle(task: &TaskMessage) -> TaskResponse {
    TaskResponse::failed(task.id, "kill: unsupported platform")
}

// ── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kill_invalid_pid() {
        let task = TaskMessage {
            command: "kill".into(),
            parameters: r#"{"pid": 0}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("error"));
    }

    #[test]
    fn test_kill_nonexistent_pid() {
        // Pick a pid almost certainly not in use. PID 1 is init on
        // Linux but we cannot kill it; use a high pid (close to the
        // max) on Unix where the syscall is available.
        #[cfg(unix)]
        {
            let task = TaskMessage {
                command: "kill".into(),
                parameters: r#"{"pid": 2147483646}"#.into(),
                ..Default::default()
            };
            let resp = handle(&task);
            // Either "already exited" (success — ESRCH treated as ok)
            // or an error string from the kernel.
            assert!(resp.completed == Some(true));
        }
        #[cfg(windows)]
        {
            // Windows path can't be tested in this harness — it requires
            // an actual process handle. Just exercise the parser.
            let task = TaskMessage {
                command: "kill".into(),
                parameters: r#"{"pid": -1}"#.into(),
                ..Default::default()
            };
            let resp = handle(&task);
            assert_eq!(resp.status.as_deref(), Some("error"));
        }
    }
}
