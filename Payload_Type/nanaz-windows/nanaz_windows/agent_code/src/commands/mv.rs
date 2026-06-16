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
    let actual_dst = if dst.is_dir() {
        dst.join(src.file_name().unwrap_or_default())
    } else {
        dst.to_path_buf()
    };

    // Create parent dirs of dst if needed
    if let Some(parent) = actual_dst.parent()
        && !parent.as_os_str().is_empty()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return TaskResponse::failed(
            task.id,
            &format!("create parent dir {} failed: {e}", display_path(parent)),
        );
    }

    match std::fs::rename(src, &actual_dst) {
        Ok(_) => TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!(
                "moved {} -> {}",
                display_path(src),
                display_path(&actual_dst)
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
                    artifact: display_path(&actual_dst),
                    needs_cleanup: true,
                    resolved: true,
                },
            ],
            ..Default::default()
        },
        Err(e) if is_cross_device(&e) => {
            // Filesystem boundary — fall back to copy + delete
            let temp_dst = temp_path_for(&actual_dst, task.id);
            match std::fs::copy(src, &temp_dst) {
                Ok(n) => {
                    if let Err(e) = replace_with_temp(&temp_dst, &actual_dst) {
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
                                display_path(&actual_dst),
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
                                    artifact: display_path(&actual_dst),
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
                                display_path(&actual_dst)
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
                            display_path(&actual_dst)
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
                display_path(&actual_dst)
            ),
        ),
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
        p.push(format!("nanaz-mv-test-{label}-{pid}-{n}"));
        std::fs::create_dir_all(&p).expect("create temp dir");
        p
    }

    #[test]
    fn test_mv_file() {
        let dir = unique_tmp("file");
        let src = dir.join("src.txt");
        let dst = dir.join("dst.txt");
        std::fs::write(&src, b"moved").unwrap();
        let task = TaskMessage {
            command: "mv".into(),
            parameters: serde_json::json!({
                "src": src.to_string_lossy(),
                "dst": dst.to_string_lossy(),
            })
            .to_string(),
            ..Default::default()
        };

        let resp = handle(&task);

        assert_eq!(resp.status.as_deref(), Some("completed"));
        assert!(!src.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"moved");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_mv_into_existing_directory() {
        let dir = unique_tmp("dir-target");
        let src = dir.join("src.txt");
        let dst_dir = dir.join("existing");
        let dst = dst_dir.join("src.txt");
        std::fs::create_dir_all(&dst_dir).unwrap();
        std::fs::write(&src, b"moved into dir").unwrap();
        let task = TaskMessage {
            command: "mv".into(),
            parameters: serde_json::json!({
                "src": src.to_string_lossy(),
                "dst": dst_dir.to_string_lossy(),
            })
            .to_string(),
            ..Default::default()
        };

        let resp = handle(&task);

        assert_eq!(resp.status.as_deref(), Some("completed"));
        assert!(!src.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"moved into dir");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
