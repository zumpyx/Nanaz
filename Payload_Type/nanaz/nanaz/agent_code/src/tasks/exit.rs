use core::sync::atomic::Ordering;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::{EXIT_PROCESS, SHOULD_EXIT};

#[derive(Deserialize)]
struct Params {
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
        "process" => {
            println!("[exit] scheduling process termination after response flush");
            SHOULD_EXIT.store(true, Ordering::Relaxed);
            EXIT_PROCESS.store(true, Ordering::Relaxed);
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some("agent process exiting".into()),
                ..Default::default()
            }
        }
        "thread" => {
            println!("[exit] stopping beacon loop");
            SHOULD_EXIT.store(true, Ordering::Relaxed);
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some("beacon loop stopped (thread exit)".into()),
                ..Default::default()
            }
        }
        other => TaskResponse::failed(task.id, &format!("unknown exit method: {other}")),
    }
}
