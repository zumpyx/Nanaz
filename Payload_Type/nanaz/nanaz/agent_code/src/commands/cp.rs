//! Copy a file — cross-platform via std::fs::copy.
//!
//! Both src and dst are protected. Without the src guard, a
//! `cp /etc/shadow /tmp/leak` would happily exfiltrate the file (the
//! dst path is /tmp, not protected), defeating the spirit of the
//! path-protection subsystem. Both ends must opt in independently.

use std::path::Path;

use mythic::{Artifact, TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::pathguard::{display_path, is_protected_path, normalize_user_path};

#[derive(Deserialize)]
struct Params {
    src: String,
    dst: String,
    /// When true, allow writing to system paths (default false).
    #[serde(default)]
    allow_system_path: bool,
    /// When true, allow reading from system paths (default false).
    /// Independent of `allow_system_path` because an operator might
    /// legitimately want to back up a config file to /tmp without
    /// being able to write back into the protected area.
    #[serde(default)]
    allow_source_system_path: bool,
}

fn temp_path_for(dest: &Path, task_id: uuid::Uuid) -> std::path::PathBuf {
    let mut name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "copy".into());
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

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("cp parse error: {e}")),
    };

    // Dst side — refuse to write into protected trees.
    let src_str = normalize_user_path(&params.src);
    let dst_str = normalize_user_path(&params.dst);

    if !params.allow_system_path && is_protected_path(&dst_str) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing to write to system path {}; set allow_system_path=true to override",
                dst_str
            ),
        );
    }
    // Src side — refuse to *read* from protected trees unless
    // explicitly opted in. The two flags are independent so the
    // operator can copy a system file out (read-ok, write-not-ok)
    // without opening the bidirectional back door.
    if !params.allow_source_system_path && is_protected_path(&src_str) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing to read from system path {}; set allow_source_system_path=true to override",
                src_str
            ),
        );
    }

    let src = Path::new(&src_str);
    let dst = Path::new(&dst_str);

    // If dst is a directory, copy into it with the same filename
    let actual_dst = if dst.is_dir() {
        dst.join(src.file_name().unwrap_or_default())
    } else if let Some(parent) = dst.parent() {
        if !parent.as_os_str().is_empty()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return TaskResponse::failed(
                task.id,
                &format!("create parent dir {} failed: {e}", display_path(parent)),
            );
        }
        dst.to_path_buf()
    } else {
        dst.to_path_buf()
    };

    let temp_dst = temp_path_for(&actual_dst, task.id);
    match std::fs::copy(src, &temp_dst) {
        Ok(n) => {
            if let Err(e) = replace_with_temp(&temp_dst, &actual_dst) {
                let _ = std::fs::remove_file(&temp_dst);
                return TaskResponse::failed(task.id, &e);
            }
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(format!(
                    "copied {} -> {} ({} bytes)",
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
                ],
                ..Default::default()
            }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&temp_dst);
            TaskResponse::failed(
                task.id,
                &format!(
                    "copy {} -> {} failed: {e}",
                    display_path(src),
                    display_path(&actual_dst)
                ),
            )
        }
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
        p.push(format!("nanaz-cp-test-{label}-{pid}-{n}"));
        std::fs::create_dir_all(&p).expect("create temp dir");
        p
    }

    #[test]
    fn test_cp_file() {
        let dir = unique_tmp("file");
        let src = dir.join("src.txt");
        let dst = dir.join("dst.txt");
        std::fs::write(&src, b"copied").unwrap();
        let task = TaskMessage {
            command: "cp".into(),
            parameters: serde_json::json!({
                "src": src.to_string_lossy(),
                "dst": dst.to_string_lossy(),
            })
            .to_string(),
            ..Default::default()
        };

        let resp = handle(&task);

        assert_eq!(resp.status.as_deref(), Some("completed"));
        assert_eq!(std::fs::read(&dst).unwrap(), b"copied");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cp_failure_preserves_existing_destination() {
        let dir = unique_tmp("preserve");
        let src = dir.join("missing.txt");
        let dst = dir.join("dst.txt");
        std::fs::write(&dst, b"original").unwrap();
        let task = TaskMessage {
            command: "cp".into(),
            parameters: serde_json::json!({
                "src": src.to_string_lossy(),
                "dst": dst.to_string_lossy(),
            })
            .to_string(),
            ..Default::default()
        };

        let resp = handle(&task);

        assert_eq!(resp.status.as_deref(), Some("error"));
        assert_eq!(std::fs::read(&dst).unwrap(), b"original");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
