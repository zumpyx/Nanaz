//! Print the agent process's current working directory.
//!
//! Task parameters: none (`{}`).
//!
//! Response:
//! - `user_output`: the cwd as a plain string for the operator.
//! - `process_response.cwd`: the same cwd for the Python wrapper to mirror
//!   into Mythic's callback state.

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;
use serde_json::json;

use crate::common::pathguard::display_path;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Params {}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let parameters = task.parameters.trim();
    let parameters = if parameters.is_empty() {
        "{}"
    } else {
        parameters
    };
    if let Err(e) = serde_json::from_str::<Params>(parameters) {
        return TaskResponse::failed(task.id, &format!("pwd parse error: {e}"));
    }

    let cwd = match std::env::current_dir() {
        Ok(p) => display_path(&p),
        Err(e) => return TaskResponse::failed(task.id, &format!("pwd failed: {e}")),
    };

    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(cwd.clone()),
        process_response: Some(json!({ "cwd": cwd })),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pwd_current_dir() {
        let task = TaskMessage {
            command: "pwd".into(),
            parameters: "{}".into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
        let expected = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(resp.user_output.as_deref(), Some(expected.as_str()));
        assert_eq!(
            resp.process_response
                .as_ref()
                .and_then(|v| v["cwd"].as_str()),
            Some(expected.as_str())
        );
    }

    #[test]
    fn test_pwd_accepts_empty_parameters() {
        let task = TaskMessage {
            command: "pwd".into(),
            parameters: "".into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
    }

    #[test]
    fn test_pwd_rejects_arguments() {
        let task = TaskMessage {
            command: "pwd".into(),
            parameters: r#"{"path":"/tmp"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("error"));
    }
}
