//! Move/rename a file — cross-platform via std::fs::rename.
//! Falls back to copy + delete when crossing filesystem boundaries.

use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize)]
struct Params {
    src: String,
    dst: String,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("mv parse error: {e}")),
    };

    let src = Path::new(&params.src);
    let dst = Path::new(&params.dst);

    // Create parent dirs of dst if needed
    if let Some(parent) = dst.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    match std::fs::rename(src, dst) {
        Ok(_) => TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!("moved {} → {}", src.display(), dst.display())),
            ..Default::default()
        },
        Err(e) if e.raw_os_error() == Some(18) /* EXDEV: cross-device link */ => {
            // Filesystem boundary — fall back to copy + delete
            match std::fs::copy(src, dst) {
                Ok(n) => match std::fs::remove_file(src) {
                    Ok(_) => TaskResponse {
                        task_id: task.id,
                        completed: Some(true),
                        status: Some("completed".into()),
                        user_output: Some(format!(
                            "moved (copy+delete) {} → {} ({} bytes)",
                            src.display(),
                            dst.display(),
                            n
                        )),
                        ..Default::default()
                    },
                    Err(e) => TaskResponse::failed(
                        task.id,
                        &format!(
                            "copied {} → {} but failed to remove source: {e}",
                            src.display(),
                            dst.display()
                        ),
                    ),
                },
                Err(e) => TaskResponse::failed(
                    task.id,
                    &format!(
                        "rename and copy both failed for {} → {}: {e}",
                        src.display(),
                        dst.display()
                    ),
                ),
            }
        }
        Err(e) => TaskResponse::failed(
            task.id,
            &format!("rename {} → {} failed: {e}", src.display(), dst.display()),
        ),
    }
}
