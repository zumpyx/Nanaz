use core::sync::atomic::Ordering;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

use crate::{EXIT_PROCESS, SHOULD_EXIT};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    /// "process" terminates the entire agent process after flushing the
    /// pending response.
    #[serde(default = "default_method")]
    method: String,
}

fn default_method() -> String {
    "process".into()
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let parameters = task.parameters.trim();
    let parameters = if parameters.is_empty() {
        "{}"
    } else {
        parameters
    };
    let method = match serde_json::from_str::<Params>(parameters) {
        Ok(p) => p.method,
        Err(e) => return TaskResponse::failed(task.id, &format!("exit parse error: {e}")),
    };

    match method.as_str() {
        "process" => {
            info!("[exit] scheduling process termination after response flush");
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
            info!("[exit] scheduling agent loop stop after response flush");
            SHOULD_EXIT.store(true, Ordering::Relaxed);
            EXIT_PROCESS.store(false, Ordering::Relaxed);
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some("agent thread exiting".into()),
                ..Default::default()
            }
        }
        other => TaskResponse::failed(task.id, &format!("unknown exit method: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EXIT_PROCESS, SHOULD_EXIT};

    fn reset_exit_flags() {
        SHOULD_EXIT.store(false, Ordering::Relaxed);
        EXIT_PROCESS.store(false, Ordering::Relaxed);
    }

    #[test]
    fn malformed_exit_parameters_do_not_exit() {
        reset_exit_flags();
        let task = TaskMessage {
            command: "exit".into(),
            parameters: "{not-json".into(),
            ..Default::default()
        };

        let resp = handle(&task);

        assert_eq!(resp.status.as_deref(), Some("error"));
        assert!(!SHOULD_EXIT.load(Ordering::Relaxed));
        assert!(!EXIT_PROCESS.load(Ordering::Relaxed));
    }

    #[test]
    fn empty_exit_parameters_default_to_process_exit() {
        reset_exit_flags();
        let task = TaskMessage {
            command: "exit".into(),
            parameters: "".into(),
            ..Default::default()
        };

        let resp = handle(&task);

        assert_eq!(resp.status.as_deref(), Some("completed"));
        assert!(SHOULD_EXIT.load(Ordering::Relaxed));
        assert!(EXIT_PROCESS.load(Ordering::Relaxed));
        reset_exit_flags();
    }
}
