//! Copy a file — cross-platform via std::fs::copy.
//!
//! Both src and dst are protected. Without the src guard, a
//! `cp /etc/shadow /tmp/leak` would happily exfiltrate the file (the
//! dst path is /tmp, not protected), defeating the spirit of the
//! path-protection subsystem. Both ends must opt in independently.

use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::pathguard::is_protected_path;

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

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("cp parse error: {e}")),
    };

    // Dst side — refuse to write into protected trees.
    if !params.allow_system_path && is_protected_path(&params.dst) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing to write to system path {}; set allow_system_path=true to override",
                params.dst
            ),
        );
    }
    // Src side — refuse to *read* from protected trees unless
    // explicitly opted in. The two flags are independent so the
    // operator can copy a system file out (read-ok, write-not-ok)
    // without opening the bidirectional back door.
    if !params.allow_source_system_path && is_protected_path(&params.src) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing to read from system path {}; set allow_source_system_path=true to override",
                params.src
            ),
        );
    }

    let src = Path::new(&params.src);
    let dst = Path::new(&params.dst);

    // If dst is a directory, copy into it with the same filename
    let actual_dst = if dst.is_dir() {
        dst.join(src.file_name().unwrap_or_default())
    } else if let Some(parent) = dst.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
        dst.to_path_buf()
    } else {
        dst.to_path_buf()
    };

    match std::fs::copy(src, &actual_dst) {
        Ok(n) => TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!("copied {} → {} ({} bytes)", src.display(), actual_dst.display(), n)),
            ..Default::default()
        },
        Err(e) => TaskResponse::failed(task.id, &format!("copy {} → {} failed: {e}", src.display(), actual_dst.display())),
    }
}
