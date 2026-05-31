//! File browser listing — used by Mythic's file browser UI.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "path": "/home/user",
//!     "host": "optional-hostname"
//! }
//! ```
//!
//! Response: `TaskResponse.file_browser` with a [`FileBrowserEntry`] tree.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use mythic::{FileBrowserEntry, TaskMessage, TaskResponse};
use serde::Deserialize;
use serde_json::Value;

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    path: String,
    #[serde(default)]
    #[allow(dead_code)]
    host: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────

/// Convert `SystemTime` to milliseconds since Unix epoch.
fn to_millis(t: std::time::SystemTime) -> Option<i64> {
    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as i64)
}

/// Build a `Value` representing Unix-style permissions (or empty on Windows).
#[cfg(unix)]
fn permissions_value(meta: &fs::Metadata) -> Option<Value> {
    use std::os::unix::fs::PermissionsExt;
    let mode = meta.permissions().mode();
    // Keep only lower 12 bits: setuid/setgid/sticky + rwx for u/g/o
    let mode = mode & 0o7777;
    let s = format!("{mode:04o}");
    // Build a dict compatible with Mythic's expected format
    Some(serde_json::json!({
        "mode": s,
        "readable": !meta.permissions().readonly(),
        "writable": true, // approximate; fine for file browser
    }))
}

#[cfg(not(unix))]
fn permissions_value(_meta: &fs::Metadata) -> Option<Value> {
    None
}

// ── Main handler ────────────────────────────────────────────

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("ls parse error: {e}")),
    };

    let path = Path::new(&params.path);

    // Resolve ~ to home dir
    let resolved = if params.path.starts_with('~') {
        if let Ok(home) = std::env::var("HOME") {
            Path::new(&home).join(params.path.trim_start_matches("~/"))
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };

    let meta = match fs::metadata(&resolved) {
        Ok(m) => m,
        Err(e) => {
            return TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("error".into()),
                user_output: Some(format!("cannot access {}: {e}", resolved.display())),
                file_browser: Some(FileBrowserEntry {
                    is_file: false,
                    name: resolved
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| params.path.clone()),
                    parent_path: resolved
                        .parent()
                        .map(|p| p.to_string_lossy().to_string()),
                    success: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            };
        }
    };

    if meta.is_file() {
        // Single file listing
        let entry = FileBrowserEntry {
            is_file: true,
            name: resolved
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| params.path.clone()),
            parent_path: resolved
                .parent()
                .map(|p| p.to_string_lossy().to_string()),
            size: Some(meta.len() as i64),
            access_time: meta.accessed().ok().and_then(to_millis),
            modify_time: meta.modified().ok().and_then(to_millis),
            permissions: permissions_value(&meta),
            success: Some(true),
            set_as_user_output: true,
            ..Default::default()
        };
        TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            file_browser: Some(entry),
            ..Default::default()
        }
    } else {
        // Directory listing
        let children = match fs::read_dir(&resolved) {
            Ok(rd) => rd,
            Err(e) => {
                return TaskResponse::failed(
                    task.id,
                    &format!("read_dir {} failed: {e}", resolved.display()),
                );
            }
        };

        let mut files: Vec<FileBrowserEntry> = Vec::new();
        for entry in children.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_file = entry
                .file_type()
                .map(|ft| ft.is_file() || (!ft.is_dir() && !ft.is_symlink()))
                .unwrap_or(true); // assume file if we can't tell
            let child_meta = entry.metadata().ok();

            files.push(FileBrowserEntry {
                is_file,
                name,
                size: child_meta.as_ref().map(|m| m.len() as i64),
                access_time: child_meta
                    .as_ref()
                    .and_then(|m| m.accessed().ok())
                    .and_then(to_millis),
                modify_time: child_meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(to_millis),
                permissions: child_meta.as_ref().and_then(|m| permissions_value(m)),
                ..Default::default()
            });
        }

        // Sort: directories first, then files, both alphabetical
        files.sort_by(|a, b| {
            b.is_file.cmp(&a.is_file)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        // Build formatted output for the interact window
        let dir_label = resolved.display().to_string();
        let mut out = format!("Listing: {dir_label}\n");
        for f in &files {
            let icon = if f.is_file { "📄" } else { "📁" };
            let size = f.size.map_or("-".into(), |s| {
                if s < 1024 { format!("{s}B") }
                else if s < 1048576 { format!("{}KB", s/1024) }
                else { format!("{}MB", s/1048576) }
            });
            out.push_str(&format!("  {icon}  {:<40}  {size:>8}\n", f.name));
        }
        out.push_str(&format!("── {} entries ──", files.len()));

        TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(out),
            file_browser: Some(FileBrowserEntry {
                is_file: false,
                name: resolved
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| params.path.clone()),
                parent_path: resolved
                    .parent()
                    .map(|p| p.to_string_lossy().to_string()),
                success: Some(true),
                set_as_user_output: true,
                files,
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ls_current_dir() {
        let task = TaskMessage {
            command: "ls".into(),
            parameters: r#"{"path": "."}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert!(resp.completed == Some(true));
        assert!(resp.file_browser.is_some());
        let fb = resp.file_browser.unwrap();
        assert!(fb.success == Some(true));
        assert!(!fb.files.is_empty());
    }

    #[test]
    fn test_ls_nonexistent() {
        let task = TaskMessage {
            command: "ls".into(),
            parameters: r#"{"path": "/nonexistent_path_xyz"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert!(resp.file_browser.is_some());
        let fb = resp.file_browser.unwrap();
        assert!(fb.success == Some(false));
    }
}
