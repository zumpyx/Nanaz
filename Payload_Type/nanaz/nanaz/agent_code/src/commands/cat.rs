//! Read a file and return its contents — cross-platform via std::fs::read.
//!
//! `user_output` is a single text blob that the Mythic UI shows as the
//! task's stdout. Without a size cap, a stray `cat /var/log/syslog` on
//! a busy server would either OOM the agent or produce a multi-hundred
//! MB Mythic response that the UI truncates ungracefully. We cap at
//! 16 MiB by default (enough for source code, configs, certificate
//! blobs; well under any reasonable tasking-pane rendering budget)
//! and stream-chunk anything larger by emitting prefix / middle / tail
//! windows instead of the whole file.
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "path": "/etc/issue",
//!     "allow_system_path": false,   // optional, default false
//!     "max_bytes": 16777216         // optional, override cap (default 16 MiB)
//! }
//! ```

use std::io::Read;
use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::pathguard::is_protected_path;
use crate::sys::encoding::decode_output;

/// Default cap (in bytes) on the size of a file `cat` will fully
/// materialise. 16 MiB is the largest size that fits comfortably in
/// Mythic's user_output field without truncating; for files above
/// that, we emit head + tail windows so the operator still sees
/// useful content.
const DEFAULT_MAX_BYTES: u64 = 16 * 1024 * 1024;
/// How many bytes of head and tail to show when a file exceeds the
/// cap. 64 KiB each is enough for the operator to recognise the file
/// contents and decide whether to download it whole.
const HEAD_BYTES: usize = 64 * 1024;
const TAIL_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
struct Params {
    path: String,
    /// When true, allow reading system paths (default false).
    #[serde(default)]
    allow_system_path: bool,
    /// Override the size cap. Clamped to [1 KiB, 256 MiB].
    #[serde(default)]
    max_bytes: Option<u64>,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("cat parse error: {e}")),
    };

    if !params.allow_system_path && is_protected_path(&params.path) {
        return TaskResponse::failed(
            task.id,
            &format!(
                "refusing to read system path {}; set allow_system_path=true to override",
                params.path
            ),
        );
    }

    let path = Path::new(&params.path);

    // Look up the size up front so we can pick the right path: full
    // read for small files, head+tail window for large.
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("stat {} failed: {e}", path.display()),
            )
        }
    };
    let cap = params
        .max_bytes
        .unwrap_or(DEFAULT_MAX_BYTES)
        .clamp(1024, 256 * 1024 * 1024);
    if meta.len() > cap {
        return emit_head_tail(task, path, meta.len());
    }

    match std::fs::read(path) {
        Ok(data) => {
            let content = decode_output(&data);
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(content),
                ..Default::default()
            }
        }
        Err(e) => TaskResponse::failed(task.id, &format!("read {} failed: {e}", path.display())),
    }
}

/// For oversized files, emit the first HEAD_BYTES and the last
/// TAIL_BYTES with an explanatory marker in between. The operator
/// still gets a meaningful snapshot of the file; if they need the
/// whole thing they can `download` it instead.
fn emit_head_tail(task: &TaskMessage, path: &Path, total: u64) -> TaskResponse {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("open {} failed: {e}", path.display()),
            )
        }
    };

    // Read the head.
    let mut head = vec![0u8; HEAD_BYTES.min(usize::try_from(total).unwrap_or(HEAD_BYTES))];
    let head_n = match file.read(&mut head) {
        Ok(n) => n,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("read head of {} failed: {e}", path.display()),
            )
        }
    };
    head.truncate(head_n);

    // Seek to the tail. `seek` to (total - TAIL_BYTES) only makes
    // sense when total > TAIL_BYTES, which is guaranteed because we
    // only enter this branch when total > cap (16 MiB) > TAIL_BYTES.
    use std::io::{Seek, SeekFrom};
    let tail_start = total.saturating_sub(TAIL_BYTES as u64);
    if let Err(e) = file.seek(SeekFrom::Start(tail_start)) {
        return TaskResponse::failed(
            task.id,
            &format!("seek {} to tail failed: {e}", path.display()),
        );
    }
    let mut tail = vec![0u8; TAIL_BYTES];
    let tail_n = match file.read(&mut tail) {
        Ok(n) => n,
        Err(e) => {
            return TaskResponse::failed(
                task.id,
                &format!("read tail of {} failed: {e}", path.display()),
            )
        }
    };
    tail.truncate(tail_n);

    let head_str = decode_output(&head);
    let tail_str = decode_output(&tail);
    let skipped = total.saturating_sub(head_n as u64 + tail_n as u64);
    let body = format!(
        "---- head ({} bytes) ----\n{}\n---- skipped {} bytes ----\n---- tail ({} bytes) ----\n{}\n",
        head_n, head_str, skipped, tail_n, tail_str
    );

    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(body),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn unique_tmp(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let mut p = std::env::temp_dir();
        p.push(format!("nanaz-cat-test-{label}-{pid}-{n}"));
        std::fs::create_dir_all(&p).expect("create temp dir");
        p
    }

    #[test]
    fn test_cat_small_file() {
        let dir = unique_tmp("small");
        let f = dir.join("hi.txt");
        std::fs::write(&f, b"hello world").unwrap();
        let task = TaskMessage {
            command: "cat".into(),
            parameters: serde_json::json!({ "path": f.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
        assert!(resp.user_output.as_deref().unwrap_or("").contains("hello world"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cat_missing_file() {
        let task = TaskMessage {
            command: "cat".into(),
            parameters: r#"{"path": "/nonexistent_xyz_123"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("error"));
    }

    #[test]
    fn test_cat_oversized_shows_head_and_tail() {
        let dir = unique_tmp("big");
        let f = dir.join("big.bin");
        // 32 MiB — bigger than the 16 MiB cap.
        let mut h = std::fs::File::create(&f).unwrap();
        let buf = vec![b'A'; 1024 * 1024];
        for _ in 0..32 {
            h.write_all(&buf).unwrap();
        }
        drop(h);
        let task = TaskMessage {
            command: "cat".into(),
            parameters: serde_json::json!({ "path": f.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
        let body = resp.user_output.unwrap();
        assert!(body.contains("head"), "expected head marker in {:?}", body);
        assert!(body.contains("tail"), "expected tail marker in {:?}", body);
        assert!(body.contains("skipped"), "expected skipped marker");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
