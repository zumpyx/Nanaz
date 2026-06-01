//! Remove a file or directory — cross-platform via std::fs.
//!
//! Recursive deletion (`rm -r`) is doubly dangerous: a single typo can
//! clobber an entire tree. Refuse to operate on system paths unless the
//! operator sets `allow_system_path: true`, and require an extra
//! `confirm_destructive: true` flag for recursive deletes against any
//! non-system path (an opt-in second factor — operators who have to
//! type two flags for a destructive op make fewer mistakes).

use std::path::Path;

use mythic::{RemovedFileInfo, TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::pathguard::is_protected_path;

#[derive(Deserialize)]
struct Params {
    path: String,
    #[serde(default)]
    recursive: bool,
    /// When true, allow deleting system paths (default false).
    #[serde(default)]
    allow_system_path: bool,
    /// When true, allow recursive deletion (default false). Required for
    /// `rm -r` regardless of the path being a system path.
    #[serde(default)]
    confirm_destructive: bool,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("rm parse error: {e}")),
    };

    let is_system = is_protected_path(&params.path);
    if is_system && !params.allow_system_path {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing to remove system path {}; set allow_system_path=true to override",
                params.path
            ),
        );
    }
    if params.recursive && !params.confirm_destructive {
        return TaskResponse::failed(
            task.id,
            "recursive removal requires confirm_destructive=true",
        );
    }

    let path = Path::new(&params.path);
    match std::fs::metadata(path) {
        Ok(meta) => {
            let result = if meta.is_dir() {
                if params.recursive {
                    std::fs::remove_dir_all(path)
                } else {
                    return TaskResponse::failed(
                        task.id,
                        &format!("{} is a directory; use recursive=true", path.display()),
                    );
                }
            } else {
                std::fs::remove_file(path)
            };

            match result {
                Ok(_) => TaskResponse {
                    task_id: task.id,
                    completed: Some(true),
                    status: Some("completed".into()),
                    user_output: Some(format!("removed {}", path.display())),
                    removed_files: vec![RemovedFileInfo {
                        host: String::new(),
                        path: params.path.clone(),
                    }],
                    ..Default::default()
                },
                Err(e) => {
                    TaskResponse::failed(task.id, &format!("remove {} failed: {e}", path.display()))
                }
            }
        }
        Err(e) => TaskResponse::failed(task.id, &format!("{}: {e}", path.display())),
    }
}
