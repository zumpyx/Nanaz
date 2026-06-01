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
    recursive: bool,
    #[serde(default)]
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

// ── Listing helpers ────────────────────────────────────────

/// Build a FileBrowserEntry for a single directory entry.
fn entry_from_direntry(
    entry: &fs::DirEntry,
    path_prefix: &str,
) -> Option<FileBrowserEntry> {
    let name = entry.file_name().to_string_lossy().to_string();
    let is_file = entry
        .file_type()
        .map(|ft| ft.is_file() || (!ft.is_dir() && !ft.is_symlink()))
        .unwrap_or(true);
    let child_meta = entry.metadata().ok();
    let display_name = if path_prefix.is_empty() {
        name
    } else {
        format!("{path_prefix}/{name}")
    };
    Some(FileBrowserEntry {
        is_file,
        name: display_name,
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
    })
}

/// Flat listing: immediate children only.
fn list_dir_flat(dir: &Path) -> Result<Vec<FileBrowserEntry>, String> {
    let rd = fs::read_dir(dir)
        .map_err(|e| format!("read_dir {} failed: {e}", dir.display()))?;
    let mut files = Vec::new();
    for entry in rd.flatten() {
        if let Some(fb) = entry_from_direntry(&entry, "") {
            files.push(fb);
        }
    }
    Ok(files)
}

/// Recursive walk: collect all files and dirs with relative paths.
fn walk_dir_recursive(root: &Path) -> Result<Vec<FileBrowserEntry>, String> {
    let mut files = Vec::new();
    let mut dirs: Vec<(std::path::PathBuf, String)> = Vec::new();
    dirs.push((root.to_path_buf(), String::new()));

    while let Some((dir, prefix)) = dirs.pop() {
        let rd = fs::read_dir(&dir)
            .map_err(|e| format!("read_dir {} failed: {e}", dir.display()))?;
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(fb) = entry_from_direntry(&entry, &prefix) {
                let is_file = fb.is_file;
                files.push(fb);
                if !is_file {
                    let child_prefix = if prefix.is_empty() {
                        name
                    } else {
                        format!("{prefix}/{name}")
                    };
                    dirs.push((entry.path(), child_prefix));
                }
            }
        }
    }

    Ok(files)
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

    let host = params.host.filter(|h| !h.is_empty());
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
            host,
            size: Some(meta.len() as i64),
            access_time: meta.accessed().ok().and_then(to_millis),
            modify_time: meta.modified().ok().and_then(to_millis),
            permissions: permissions_value(&meta),
            success: Some(true),
            update_deleted: true,
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
        // Directory listing — walk recursively if requested
        let children = if params.recursive {
            walk_dir_recursive(&resolved)
        } else {
            list_dir_flat(&resolved)
        };
        let files = match children {
            Ok(f) => f,
            Err(e) => return TaskResponse::failed(task.id, &e),
        };

        // Sort: directories first, then files, both alphabetical
        let mut files = files;
        files.sort_by(|a, b| {
            b.is_file.cmp(&a.is_file)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let count = files.len();
        TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!("{count} entries")),
            file_browser: Some(FileBrowserEntry {
                is_file: false,
                name: resolved
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| params.path.clone()),
                parent_path: resolved
                    .parent()
                    .map(|p| p.to_string_lossy().to_string()),
                host,
                success: Some(true),
                update_deleted: true,
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
