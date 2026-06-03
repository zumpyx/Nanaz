//! Network connection listing — cross-platform.
//!
//! Linux: parses /proc/net/tcp, /proc/net/udp
//! macOS: uses `lsof -i -n -P` or `netstat -an`
//! Windows: uses `netstat -ano`

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;
use serde::Serialize;

#[allow(unused_imports)]
use crate::sys::encoding::decode_output;

// ── Output struct ───────────────────────────────────────────

#[derive(Serialize)]
struct NetEntry {
    protocol: String,
    local_addr: String,
    local_port: u16,
    remote_addr: String,
    remote_port: u16,
    state: String,
    pid: Option<i64>,
}

// ── Linux: parse /proc/net ──────────────────────────────────

#[cfg(target_os = "linux")]
fn ip_to_string_v4(ip: u32) -> String {
    let b = ip.to_ne_bytes();
    format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
}

#[cfg(target_os = "linux")]
fn hex_to_ipv6(s: &str) -> String {
    // /proc/net/tcp6 stores IPv6 as 4 groups of 8 hex chars (32 chars total), big-endian
    if s.len() != 32 {
        return "::".into();
    }
    let mut groups = Vec::new();
    for i in (0..32).step_by(4) {
        groups.push(u16::from_str_radix(&s[i..i + 4], 16).unwrap_or(0));
    }
    // Format as compressed IPv6
    format!(
        "{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
        groups[0], groups[1], groups[2], groups[3], groups[4], groups[5], groups[6], groups[7],
    )
}

#[cfg(target_os = "linux")]
fn hex_to_ip(s: &str) -> (String, u16) {
    // /proc/net/tcp:  "0100007F:0050" (8+4 hex chars, IPv4)
    // /proc/net/tcp6: "0000000000000000FFFF00000100007F:0050" (32+4, IPv6 or v4-mapped)
    let colon = s.rfind(':').unwrap_or(s.len());
    let addr = &s[..colon];
    let port = u16::from_str_radix(&s[colon + 1..], 16).unwrap_or(0);

    let ip = match addr.len() {
        8 => {
            // IPv4
            u32::from_str_radix(addr, 16)
                .map(|n| ip_to_string_v4(n.swap_bytes()))
                .unwrap_or_else(|_| "0.0.0.0".into())
        }
        32 => {
            // IPv6 (possibly IPv4-mapped)
            hex_to_ipv6(addr)
        }
        _ => "0.0.0.0".into(),
    };
    (ip, port)
}

#[cfg(target_os = "linux")]
fn parse_proc_net(path: &str, protocol: &str) -> Vec<NetEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 10 {
            continue;
        }
        let (local_addr, local_port) = hex_to_ip(parts[1]);
        let (remote_addr, remote_port) = hex_to_ip(parts[2]);
        let state = match parts[3] {
            "01" => "ESTABLISHED",
            "02" => "SYN_SENT",
            "03" => "SYN_RECV",
            "04" => "FIN_WAIT1",
            "05" => "FIN_WAIT2",
            "06" => "TIME_WAIT",
            "07" => "CLOSE",
            "08" => "CLOSE_WAIT",
            "09" => "LAST_ACK",
            "0A" => "LISTEN",
            "0B" => "CLOSING",
            _ => "UNKNOWN",
        };

        entries.push(NetEntry {
            protocol: protocol.into(),
            local_addr,
            local_port,
            remote_addr,
            remote_port,
            state: state.into(),
            pid: None, // /proc/net doesn't have PID directly, use socket inode mapping if needed
        });
    }
    entries
}

#[cfg(target_os = "linux")]
fn list_connections() -> Result<Vec<NetEntry>, String> {
    let mut entries = parse_proc_net("/proc/net/tcp", "tcp");
    entries.extend(parse_proc_net("/proc/net/tcp6", "tcp6"));
    entries.extend(parse_proc_net("/proc/net/udp", "udp"));
    entries.extend(parse_proc_net("/proc/net/udp6", "udp6"));
    Ok(entries)
}

// ── macOS: prefer netstat, fall back to lsof ────────────────
//
// Format we parse (BSD netstat):
//   Active Internet Connections (including servers)
//   Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)
//   tcp4       0      0  192.168.1.10.443       10.0.0.5.51234        ESTABLISHED
//   tcp6       0      0  fe80::1.443            fe80::2.51234         ESTABLISHED
//   udp4       0      0  *.5353                 *.*

#[cfg(target_os = "macos")]
fn list_connections() -> Result<Vec<NetEntry>, String> {
    // Try `netstat -an -W -p tcp,udp` first (built-in, no SIP impact); fall
    // back to lsof if netstat is missing on a stripped host. -W truncates
    // wide output so the columns line up.
    let output = std::process::Command::new("netstat")
        .args(["-an", "-W", "-p", "tcp,udp"])
        .output()
        .or_else(|_| {
            std::process::Command::new("lsof")
                .args(["-i", "-n", "-P"])
                .output()
        })
        .map_err(|e| format!("netstat/lsof failed: {e}"))?;

    let stdout = decode_output(&output.stdout);
    let mut entries = Vec::new();
    let mut in_active_block = false;

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Active Internet") {
            in_active_block = true;
            continue;
        }
        if !in_active_block {
            continue;
        }
        if trimmed.starts_with("Proto") {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        // netstat -W columns are space-padded but variable-width; split on
        // whitespace to get the field array.
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        // Layout: proto(0) recv-q(1) send-q(2) local(3) foreign(4) [state(5)]
        // UDP rows have no state column, so they only reach parts.len() == 5.
        if parts.len() < 5 {
            continue;
        }
        let proto = parts[0].to_lowercase();
        if !(proto.starts_with("tcp") || proto.starts_with("udp")) {
            continue;
        }
        let local = parts[3];
        let remote = parts[4];
        // UDP is connectionless — it has no state; "NONE" matches the
        // convention the Windows branch uses.
        let state = if proto.starts_with("udp") {
            "NONE".to_string()
        } else if parts.len() >= 6 {
            parts[5].to_string()
        } else {
            "UNKNOWN".to_string()
        };

        if let Some(entry) = parse_addr_pair(&proto, local, remote, &state, None) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

/// Parse a `host.port` pair from netstat-style output into local/remote fields.
#[cfg(target_os = "macos")]
fn parse_addr_pair(
    proto: &str,
    local: &str,
    remote: &str,
    state: &str,
    pid: Option<i64>,
) -> Option<NetEntry> {
    let (la, lp) = split_hostport(local)?;
    let (ra, rp) = if remote == "*.*" || remote == "*" {
        ("0.0.0.0".to_string(), 0)
    } else {
        split_hostport(remote)?
    };
    Some(NetEntry {
        protocol: proto.into(),
        local_addr: la,
        local_port: lp,
        remote_addr: ra,
        remote_port: rp,
        state: state.into(),
        pid,
    })
}

/// Split a `host.port` string from netstat into (host, port). Handles the
/// IPv6 `[::1].443` form as well as plain `127.0.0.1.80`.
#[cfg(target_os = "macos")]
fn split_hostport(s: &str) -> Option<(String, u16)> {
    // IPv6 literal: '[addr].port'  e.g. '[fe80::1].443'
    if s.starts_with('[') {
        let end = s.find("].")?;
        let host = s[1..end].to_string();
        let port: u16 = s[end + 2..].parse().ok()?;
        return Some((host, port));
    }
    // Plain 'host.port' — find the LAST dot
    let dot = s.rfind('.')?;
    let host = s[..dot].to_string();
    let port: u16 = s[dot + 1..].parse().ok()?;
    Some((host, port))
}

// ── Windows: netstat -ano ───────────────────────────────────

#[cfg(windows)]
fn list_connections() -> Result<Vec<NetEntry>, String> {
    let output = std::process::Command::new("netstat")
        .args(["-ano"])
        .output()
        .map_err(|e| format!("netstat failed: {e}"))?;

    let stdout = decode_output(&output.stdout);
    let mut entries = Vec::new();
    let mut in_tcp = false;

    for line in stdout.lines() {
        let line = line.trim();

        if line.starts_with("Active") {
            in_tcp = line.contains("TCP") || line.contains("UDP");
            continue;
        }
        if line.starts_with("Proto") {
            continue;
        }

        if in_tcp && !line.is_empty() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let proto = parts[0].to_lowercase();
                // Windows `netstat -ano` row format:
                //   TCP  0.0.0.0:80   0.0.0.0:0   LISTENING   1234   <- 5 columns
                //   UDP  0.0.0.0:123  *:*                    5678   <- 4 columns (no state)
                // The state column is the 4th token, present only for TCP.
                // Earlier we conflated it with local-address and labelled UDP
                // rows as "ESTABLISHED" — which is wrong (UDP is stateless).
                let (state, pid) = if proto.starts_with("udp") {
                    // No state column. The PID is the last token, which is
                    // parts[3] when there are exactly 4 columns.
                    (
                        "NONE".to_string(),
                        parts.last().and_then(|s| s.parse().ok()),
                    )
                } else {
                    let s = if parts.len() >= 5
                        && parts[3].chars().all(|c| c.is_alphabetic() || c == '_')
                    {
                        parts[3].to_string()
                    } else {
                        "UNKNOWN".to_string()
                    };
                    let p = parts.last().and_then(|s| s.parse().ok());
                    (s, p)
                };
                let local_parts: Vec<&str> = parts[1].rsplitn(2, ':').collect();
                let remote_parts: Vec<&str> = parts[2].rsplitn(2, ':').collect();

                if local_parts.len() >= 2 && remote_parts.len() >= 2 {
                    entries.push(NetEntry {
                        protocol: proto,
                        local_addr: local_parts[1].trim_start_matches('[').to_string(),
                        local_port: local_parts[0].trim_end_matches(']').parse().unwrap_or(0),
                        remote_addr: remote_parts[1].trim_start_matches('[').to_string(),
                        remote_port: remote_parts[0].trim_end_matches(']').parse().unwrap_or(0),
                        state,
                        pid,
                    });
                }
            }
        }
    }
    Ok(entries)
}

// ── Fallback ────────────────────────────────────────────────

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn list_connections() -> Result<Vec<NetEntry>, String> {
    Err("netstat: unsupported platform".into())
}

// ── Main handler ────────────────────────────────────────────

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
        return TaskResponse::failed(task.id, &format!("netstat parse error: {e}"));
    }
    match list_connections() {
        Ok(entries) => {
            let count = entries.len();
            let json = serde_json::to_string_pretty(&entries).unwrap_or_default();
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(format!("{count} connections\n{json}")),
                ..Default::default()
            }
        }
        Err(e) => TaskResponse::failed(task.id, &e),
    }
}
