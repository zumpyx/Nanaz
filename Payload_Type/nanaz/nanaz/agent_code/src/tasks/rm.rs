//! Remove a file or directory — cross-platform via std::fs.

use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize)]
struct Params {
    path: String,
    #[serde(default)]
    recursive: bool,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("rm parse error: {e}")),
    };

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
