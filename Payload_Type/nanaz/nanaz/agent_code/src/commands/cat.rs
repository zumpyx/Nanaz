//! Read a file and return its contents — cross-platform via std::fs::read.

use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::pathguard::is_protected_path;
use crate::sys::encoding::decode_output;

#[derive(Deserialize)]
struct Params {
    path: String,
    /// When true, allow reading system paths (default false).
    #[serde(default)]
    allow_system_path: bool,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("cat parse error: {e}")),
    };

    if !params.allow_system_path && is_protected_path(&params.path) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing to read system path {}; set allow_system_path=true to override",
                params.path
            ),
        );
    }

    let path = Path::new(&params.path);
    match std::fs::read(path) {
        Ok(data) => {
            let content = decode_output(&data);
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(content),
                ..Default::default()
            }
        }
        Err(e) => TaskResponse::failed(task.id, &format!("read {} failed: {e}", path.display())),
    }
}
