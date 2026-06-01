//! DNS resolution — cross-platform via std::net::ToSocketAddrs.

use std::net::ToSocketAddrs;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize)]
struct Params {
    hostname: String,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("resolve parse error: {e}")),
    };

    // Add port 0 for resolution (won't actually connect)
    let target = format!("{}:0", params.hostname);
    match target.to_socket_addrs() {
        Ok(addrs) => {
            let mut ips: Vec<String> = Vec::new();
            for addr in addrs {
                ips.push(addr.ip().to_string());
            }
            ips.sort();
            ips.dedup();
            let output = ips.join("\n");
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(if output.is_empty() {
                    format!("no addresses found for {}", params.hostname)
                } else {
                    output
                }),
                ..Default::default()
            }
        }
        Err(e) => TaskResponse::failed(
            task.id,
            &format!("resolve {} failed: {e}", params.hostname),
        ),
    }
}
