//! File browser listing — used by Mythic's file browser UI.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "path": "/home/user"
//! }
//! ```
//!
//! Response: `TaskResponse.file_browser` with a [`FileBrowserEntry`] tree.
//!
//! The response includes both `file_browser` for Mythic's file browser
//! and `user_output` for the interact pane.

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
    depth: Option<u32>,
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

fn format_size(n: i64) -> String {
    if n < 1024 {
        format!("{n}B")
    } else if n < 1024 * 1024 {
        format!("{}KB", n / 1024)
    } else if n < 1024 * 1024 * 1024 {
        format!("{}MB", n / (1024 * 1024))
    } else {
        format!("{}GB", n / (1024 * 1024 * 1024))
    }
}

fn listing_output(parent: &str, name: &str, files: &[FileBrowserEntry]) -> String {
    let path = join_display(parent, name);
    if files.is_empty() {
        return format!("empty: {path}");
    }

    let mut lines = Vec::with_capacity(files.len() + 2);
    lines.push(format!("Listing: {path}"));
    for file in files {
        let marker = if file.is_file { "FILE" } else { "DIR " };
        let size = format_size(file.size.unwrap_or(0));
        lines.push(format!("  {marker}  {:<40}  {:>8}", file.name, size));
    }
    lines.push(format!("-- {} entries --", files.len()));
    lines.join("\n")
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
fn walk_dir_recursive(root: &Path, max_depth: u32) -> Result<Vec<FileBrowserEntry>, String> {
    let mut files = Vec::new();
    let mut dirs: Vec<(std::path::PathBuf, String, u32)> = Vec::new();
    dirs.push((root.to_path_buf(), String::new(), 0));

    while let Some((dir, prefix, depth)) = dirs.pop() {
        if depth >= max_depth {
            continue;
        }
        let rd = fs::read_dir(&dir)
            .map_err(|e| format!("read_dir {} failed: {e}", render_path(&dir)))?;
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(fb) = entry_from_direntry(&entry, &prefix) {
                let is_file = fb.is_file;
                files.push(fb);
                let entry_depth = depth + 1;
                if !is_file && entry_depth < max_depth {
                    let child_prefix = join_display(&prefix, &name);
                    dirs.push((entry.path(), child_prefix, entry_depth));
                }
            }
        }
    }

    Ok(files)
}

// ── Main handler ────────────────────────────────────────────

pub fn handle(task: &TaskMessage) -> TaskResponse {
    handle_with_mode(task, false)
}

pub fn handle_tree(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("tree parse error: {e}")),
    };

    let input_path = normalize_user_path(&params.path);
    let resolved = resolve_path(&input_path);
    let meta = match fs::metadata(&resolved) {
        Ok(m) => m,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("cannot access {}: {e}", render_path(&resolved)),
            );
        }
    };

    if meta.is_file() {
        return TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(render_path(&resolved)),
            ..Default::default()
        };
    }

    let depth = params.depth.unwrap_or(3);
    let mut files = match walk_dir_recursive(&resolved, depth) {
        Ok(files) => files,
        Err(e) => return TaskResponse::failed(task.id, &e),
    };
    files.sort_by_key(|entry| entry.name.to_lowercase());

    let mut lines = vec![render_path(&resolved)];
    for entry in files {
        let marker = if entry.is_file { "FILE" } else { "DIR " };
        lines.push(format!("{marker} {}", entry.name));
    }

    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(lines.join("\n")),
        ..Default::default()
    }
}

fn handle_with_mode(task: &TaskMessage, recursive: bool) -> TaskResponse {
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
        let output = format!(
            "{} ({})",
            join_display(parent_path.as_deref().unwrap_or_default(), &node_name),
            format_size(meta.len() as i64)
        );
        let entry = FileBrowserEntry {
            is_file: true,
            name: node_name.clone(),
            host,
            parent_path: parent_path.clone(),
            size: Some(meta.len() as i64),
            access_time: meta.accessed().ok().and_then(to_millis),
            modify_time: meta.modified().ok().and_then(to_millis),
            permissions: permissions_value(&meta),
            success: Some(true),
            ..Default::default()
        };
        let user_output = serde_json::to_string(&entry).unwrap_or(output);
        TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(user_output),
            file_browser: Some(entry),
            ..Default::default()
        }
    } else {
        // Directory listing — tree uses recursive mode; ls stays flat.
        let children = if recursive {
            walk_dir_recursive(&resolved, params.depth.unwrap_or(3))
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

        let output = listing_output(
            parent_path.as_deref().unwrap_or_default(),
            &node_name,
            &files,
        );
        let entry = FileBrowserEntry {
            is_file: false,
            name: node_name,
            host,
            parent_path,
            success: Some(true),
            // Do not default update_deleted=true here. Mythic's
            // server-side file-browser cleanup searches by
            // operation/host/path and updates existing rows without
            // limiting to the current callback. On hosts with old
            // callbacks, that causes a fresh ls to mutate stale
            // callback trees and pollute the File Browser UI.
            files,
            ..Default::default()
        };
        let user_output = serde_json::to_string(&entry).unwrap_or(output);
        TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(user_output),
            file_browser: Some(entry),
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
        assert!(resp.user_output.unwrap_or_default().contains("hello.txt"));
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
    fn test_ls_user_output_set_without_structured_stdout_flag() {
        // Interact needs `user_output`; the doubled-output guard is
        // keeping `set_as_user_output` false on the structured payload.
        let dir = unique_tmp("no-double");
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        let task = TaskMessage {
            command: "ls".into(),
            parameters: serde_json::json!({ "path": dir.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert!(resp.user_output.unwrap_or_default().contains("a.txt"));
        let fb = resp.file_browser.expect("file_browser set");
        assert!(!fb.set_as_user_output);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ls_does_not_default_update_deleted() {
        let dir = unique_tmp("no-update-deleted");
        std::fs::write(dir.join("hello.txt"), b"hi").unwrap();
        let task = TaskMessage {
            command: "ls".into(),
            parameters: serde_json::json!({ "path": dir.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        let fb = resp.file_browser.expect("file_browser set");
        assert!(!fb.update_deleted);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ls_stays_flat_even_with_legacy_recursive_param() {
        let dir = unique_tmp("flat");
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested").join("deep.txt"), b"hi").unwrap();
        let task = TaskMessage {
            command: "ls".into(),
            parameters: serde_json::json!({
                "path": dir.to_string_lossy(),
                "recursive": true,
            })
            .to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        let fb = resp.file_browser.expect("file_browser set");
        assert!(fb.files.iter().any(|f| f.name == "nested"));
        assert!(!fb.files.iter().any(|f| f.name.contains("deep.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_tree_walks_recursively() {
        let dir = unique_tmp("tree");
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested").join("deep.txt"), b"hi").unwrap();
        let task = TaskMessage {
            command: "tree".into(),
            parameters: serde_json::json!({ "path": dir.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let resp = handle_tree(&task);
        assert!(resp.file_browser.is_none());
        let output = resp.user_output.unwrap_or_default();
        assert!(output.contains("nested"));
        assert!(output.contains("deep.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
