//! DNS resolution — cross-platform via std::net::ToSocketAddrs.
//!
//! Hostname length is capped to a DNS-legal maximum (253 bytes) so a
//! pathologically large operator-supplied string can't pin the agent in
//! the resolver.

use std::net::ToSocketAddrs;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

/// Per RFC 1035 §2.3.4, a DNS name (including dots) is at most 253 bytes.
/// We also reject empty / whitespace-only inputs early.
const MAX_HOSTNAME_LEN: usize = 253;

#[derive(Deserialize)]
struct Params {
    hostname: String,
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("resolve parse error: {e}")),
    };

    let hostname = params.hostname.trim();
    if hostname.is_empty() {
        return TaskResponse::failed(task.id, "resolve: empty hostname");
    }
    if hostname.len() > MAX_HOSTNAME_LEN {
        return TaskResponse::failed(
            task.id,
            &format!(
                "resolve: hostname exceeds {MAX_HOSTNAME_LEN} bytes (got {})",
                hostname.len()
            ),
        );
    }

    // to_socket_addrs() needs `host:port`. Port 0 is a sentinel that the
    // resolver never actually contacts. If the operator passed an IPv6
    // literal (`::1`) we need brackets; bracketing a hostname is harmless
    // and to_socket_addrs accepts it.
    let target = format!("{hostname}:0");
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
                    format!("no addresses found for {hostname}")
                } else {
                    output
                }),
                ..Default::default()
            }
        }
        Err(e) => TaskResponse::failed(task.id, &format!("resolve {hostname} failed: {e}")),
    }
}
