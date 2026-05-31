//! Create a directory — cross-platform via std::fs::create_dir_all.

use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize)]
struct Params {
    path: String,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("mkdir parse error: {e}")),
    };

    let path = Path::new(&params.path);
    match std::fs::create_dir_all(path) {
        Ok(_) => TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!("created directory {}", path.display())),
            ..Default::default()
        },
        Err(e) => TaskResponse::failed(task.id, &format!("mkdir {} failed: {e}", path.display())),
    }
}
