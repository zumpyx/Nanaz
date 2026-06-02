//! File browser listing — used by Mythic's file browser UI.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "path": "/home/user",
//!     "recursive": false
//! }
//! ```
//!
//! Response: `TaskResponse.file_browser` with a [`FileBrowserEntry`] tree.
//!
//! Notes on the `set_as_user_output` / `user_output` interaction:
//!
//! The Mythic UI in its new (file-browser) mode shows the structured
//! payload by default, not `user_output`. We deliberately leave
//! `user_output` empty here so the Python `LsCommand.process_response`
//! is the *only* writer of the human-readable table — without that
//! discipline, the operator would see two output blocks (one from
//! the structured payload, one from the user_output text) for every
//! single `ls` call. Earlier versions had this race and the resulting
//! doubled output was the most-reported UI bug in the operator chat.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::common::pathguard::{display_path as render_path, normalize_user_path};
use crate::sys::metadata;
use mythic::{FileBrowserEntry, TaskMessage, TaskResponse};
use serde::Deserialize;
use serde_json::Value;

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    #[serde(default = "default_path")]
    path: String,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    recursive: bool,
}

fn default_path() -> String {
    ".".into()
}

// ── Helpers ─────────────────────────────────────────────────

/// Convert `SystemTime` to milliseconds since Unix epoch.
fn to_millis(t: std::time::SystemTime) -> Option<i64> {
    t.duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as i64)
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

fn local_host(params: &Params) -> Option<String> {
    params
        .host
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map(|h| h.to_uppercase())
        .or_else(|| metadata::hostname().map(|h| h.to_uppercase()))
}

fn resolve_path(input: &str) -> PathBuf {
    let normalized = normalize_user_path(input);
    let trimmed = normalized.trim();
    let path = if trimmed.is_empty() || trimmed == "." {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            Path::new(&home).join(rest)
        } else {
            PathBuf::from(trimmed)
        }
    } else if trimmed == "~" {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(trimmed))
    } else {
        PathBuf::from(trimmed)
    };

    fs::canonicalize(&path).unwrap_or(path)
}

fn split_name_parent(path: &Path, original: &str) -> (String, Option<String>) {
    let parent = path.parent().map(render_path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            let shown = render_path(path);
            if shown.is_empty() {
                original.to_string()
            } else {
                shown
            }
        });

    let parent = parent.filter(|p| p != &name);
    (name, parent)
}

fn join_display(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        return name.to_string();
    }
    let sep = if cfg!(windows) || parent.contains('\\') {
        "\\"
    } else {
        "/"
    };
    if parent.ends_with('\\') || parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}{sep}{name}")
    }
}

// ── Listing helpers ────────────────────────────────────────

/// Build a FileBrowserEntry for a single directory entry.
fn entry_from_direntry(entry: &fs::DirEntry, path_prefix: &str) -> Option<FileBrowserEntry> {
    let name = entry.file_name().to_string_lossy().to_string();
    let is_file = entry
        .file_type()
        .map(|ft| ft.is_file() || (!ft.is_dir() && !ft.is_symlink()))
        .unwrap_or(true);
    let child_meta = entry.metadata().ok();
    let display_name = if path_prefix.is_empty() {
        name
    } else {
        join_display(path_prefix, &name)
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
        permissions: child_meta.as_ref().and_then(permissions_value),
        ..Default::default()
    })
}

/// Flat listing: immediate children only.
fn list_dir_flat(dir: &Path) -> Result<Vec<FileBrowserEntry>, String> {
    let rd = fs::read_dir(dir).map_err(|e| format!("read_dir {} failed: {e}", render_path(dir)))?;
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
            .map_err(|e| format!("read_dir {} failed: {e}", render_path(&dir)))?;
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(fb) = entry_from_direntry(&entry, &prefix) {
                let is_file = fb.is_file;
                files.push(fb);
                if !is_file {
                    let child_prefix = join_display(&prefix, &name);
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

    let input_path = normalize_user_path(&params.path);
    let resolved = resolve_path(&input_path);
    let host = local_host(&params);
    let (node_name, parent_path) = split_name_parent(&resolved, &input_path);

    let meta = match fs::metadata(&resolved) {
        Ok(m) => m,
        Err(e) => {
            return TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("error".into()),
                user_output: Some(format!("cannot access {}: {e}", render_path(&resolved))),
                file_browser: Some(FileBrowserEntry {
                    is_file: false,
                    name: node_name,
                    host,
                    parent_path,
                    success: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            };
        }
    };

    if meta.is_file() {
        // Single file listing — surface the entry directly so the UI
        // shows the file without us having to write a separate stdout.
        // `set_as_user_output` would force the structured payload into
        // the user_output field; combined with the Python wrapper
        // writing a second formatted table this used to produce a
        // doubled output. We leave both flags off and let the Python
        // wrapper emit the human-readable line.
        let entry = FileBrowserEntry {
            is_file: true,
            name: node_name,
            host,
            parent_path,
            size: Some(meta.len() as i64),
            access_time: meta.accessed().ok().and_then(to_millis),
            modify_time: meta.modified().ok().and_then(to_millis),
            permissions: permissions_value(&meta),
            success: Some(true),
            update_deleted: true,
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
            b.is_file
                .cmp(&a.is_file)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        // The structured `file_browser` payload is what the Mythic UI
        // consumes. The Python `LsCommand.process_response` writes a
        // human-readable table via `MythicRPCTaskUpdate`. We
        // deliberately leave `user_output` empty here and DO NOT set
        // `set_as_user_output: true` — either of those would race with
        // the Python-side formatter and produce a doubled block.
        TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: None,
            file_browser: Some(FileBrowserEntry {
                is_file: false,
                name: node_name,
                host,
                parent_path,
                success: Some(true),
                update_deleted: true,
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Per-test unique temp directory under the system temp dir. Avoids the
    /// tests being sensitive to whatever `cargo test` set as the cwd.
    fn unique_tmp(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let mut p = std::env::temp_dir();
        p.push(format!("nanaz-ls-test-{label}-{pid}-{n}"));
        std::fs::create_dir_all(&p).expect("create temp dir");
        p
    }

    #[test]
    fn test_ls_current_dir() {
        // Make a known directory with a known file so the assertion is
        // independent of the process cwd.
        let dir = unique_tmp("list");
        std::fs::write(dir.join("hello.txt"), b"hi").unwrap();

        let task = TaskMessage {
            command: "ls".into(),
            parameters: serde_json::json!({ "path": dir.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        let fb = resp.file_browser.expect("file_browser set");
        assert_eq!(fb.success, Some(true));
        assert!(
            !fb.files.is_empty(),
            "expected at least hello.txt in the listing"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ls_dot_is_canonicalized_for_file_browser() {
        let resolved = resolve_path(".");
        let (name, parent) = split_name_parent(&resolved, ".");
        assert_ne!(name, ".");
        assert!(parent.is_some());
    }

    #[test]
    fn test_ls_nonexistent() {
        let task = TaskMessage {
            command: "ls".into(),
            parameters: r#"{"path": "/nonexistent_path_xyz"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        let fb = resp.file_browser.expect("file_browser set");
        assert_eq!(fb.success, Some(false));
    }

    #[test]
    fn test_ls_user_output_not_set() {
        // Regression guard: the doubled-output bug used to occur when
        // both `set_as_user_output: true` AND the Python wrapper wrote
        // a formatted table. The fix keeps `user_output` empty here.
        let dir = unique_tmp("no-double");
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        let task = TaskMessage {
            command: "ls".into(),
            parameters: serde_json::json!({ "path": dir.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert!(
            resp.user_output.is_none(),
            "user_output must be None to avoid double-display"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
