//! Move/rename a file — cross-platform via std::fs::rename.
//! Falls back to copy + delete when crossing filesystem boundaries.
//!
//! Both src and dst are protected. Without the src guard, an
//! attacker-controlled `mv /etc/passwd /tmp/exfil` would happily
//! delete the system file even when `dst` is not protected. Both
//! ends must opt in independently.

use std::io;
use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::pathguard::{display_path, is_protected_path, normalize_user_path};

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

#[derive(Deserialize)]
struct Params {
    src: String,
    dst: String,
    /// When true, allow writing to system paths (default false).
    #[serde(default)]
    allow_system_path: bool,
    /// When true, allow deleting (renaming away from) system paths
    /// (default false). Independent of `allow_system_path` so the
    /// operator can rename a system file out (src-ok, dst-not-ok)
    /// without opening the bidirectional back door.
    #[serde(default)]
    allow_source_system_path: bool,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("mv parse error: {e}")),
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
    // Src side — refuse to *delete* (rename away from) a protected
    // path unless explicitly opted in.
    if !params.allow_source_system_path && is_protected_path(&src_str) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing to remove from system path {}; set allow_source_system_path=true to override",
                src_str
            ),
        );
    }

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
                "moved {} → {}",
                display_path(src),
                display_path(dst)
            )),
            ..Default::default()
        },
        Err(e) if is_cross_device(&e) => {
            // Filesystem boundary — fall back to copy + delete
            match std::fs::copy(src, dst) {
                Ok(n) => match std::fs::remove_file(src) {
                    Ok(_) => TaskResponse {
                        task_id: task.id,
                        completed: Some(true),
                        status: Some("completed".into()),
                        user_output: Some(format!(
                            "moved (copy+delete) {} → {} ({} bytes)",
                            display_path(src),
                            display_path(dst),
                            n
                        )),
                        ..Default::default()
                    },
                    Err(e) => TaskResponse::failed(
                        task.id,
                        &format!(
                            "copied {} → {} but failed to remove source: {e}",
                            display_path(src),
                            display_path(dst)
                        ),
                    ),
                },
                Err(e) => TaskResponse::failed(
                    task.id,
                    &format!(
                        "rename and copy both failed for {} → {}: {e}",
                        display_path(src),
                        display_path(dst)
                    ),
                ),
            }
        }
        Err(e) => TaskResponse::failed(
            task.id,
            &format!(
                "rename {} → {} failed: {e}",
                display_path(src),
                display_path(dst)
            ),
        ),
    }
}
