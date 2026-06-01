use core::sync::atomic::Ordering;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::{EXIT_PROCESS, SHOULD_EXIT};

#[derive(Deserialize)]
struct Params {
    /// "process" terminates the entire agent process after flushing the
    /// pending response. "thread" is no longer supported (the agent runs
    /// the beacon loop inline; stop-the-loop = stop-the-process). Kept in
    /// the schema for backwards-compat with old operators, but maps to
    /// process exit.
    #[serde(default = "default_method")]
    method: String,
}

fn default_method() -> String {
    "process".into()
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let method = serde_json::from_str::<Params>(&task.parameters)
        .map(|p| p.method)
        .unwrap_or_else(|_| default_method());

    match method.as_str() {
        "process" | "thread" => {
            info!("[exit] scheduling process termination after response flush");
            SHOULD_EXIT.store(true, Ordering::Relaxed);
            EXIT_PROCESS.store(true, Ordering::Relaxed);
            let note = if method == "thread" {
                " (legacy 'thread' method maps to process exit)"
            } else {
                ""
            };
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(format!("agent process exiting{note}")),
                ..Default::default()
            }
        }
        other => TaskResponse::failed(task.id, &format!("unknown exit method: {other}")),
    }
}
