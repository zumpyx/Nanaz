//! Process listing — cross-platform.
//!
//! Linux: parses `/proc` filesystem.
//! macOS: uses `ps` command.
//! Windows: uses `wmic` command (or fallback to `tasklist`).
//!
//! Task parameters: none. `ps` accepts no arguments; the operator
//! doesn't need to supply a `host` because Mythic tags each process
//! with the originating callback's host server-side.
//!
//! Response: `TaskResponse.processes` with a `Vec<ProcessEntry>`.

use std::collections::{HashMap, HashSet};

use mythic::{ProcessEntry, TaskMessage, TaskResponse};
use serde::Deserialize;
#[cfg(windows)]
use serde_json::Value;

#[allow(unused_imports)]
use crate::sys::encoding::decode_output;
use crate::sys::metadata;

// ── Params ──────────────────────────────────────────────────

/// `ps` takes no parameters. The struct is kept as a placeholder
/// for symmetry with other commands and to give `serde_json` a
/// well-defined (empty) schema to validate against.
#[derive(Deserialize, Default)]
struct Params {}

fn process_host() -> String {
    metadata::hostname()
        .map(|h| h.to_uppercase())
        .unwrap_or_default()
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

#[cfg(target_os = "linux")]
fn boot_time_millis() -> Option<u64> {
    std::fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            if parts.next()? != "btime" {
                return None;
            }
            let seconds: u64 = parts.next()?.parse().ok()?;
            Some(seconds * 1000)
        })
}

// ── Linux: parse /proc ──────────────────────────────────────

#[cfg(target_os = "linux")]
fn list_processes() -> Result<Vec<ProcessEntry>, String> {
    let mut procs: Vec<ProcessEntry> = Vec::new();
    let hz = clock_ticks_per_second();
    let boot_ms = boot_time_millis().unwrap_or(0);
    let host = process_host();

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
        let rest = if comm_end + 1 < stat.len() {
            &stat[comm_end + 2..]
        } else {
            ""
        };
        let fields: Vec<&str> = rest.split_whitespace().collect();

        let ppid: Option<i64> = fields.get(1).and_then(|s| s.parse().ok());
        // starttime is at field index 19 (0-based after comm+state removal:
        // state is field 0, then ppid=1, ..., starttime=19). The value is
        // in `_SC_CLK_TCK` units; convert to milliseconds using the live
        // tick rate.
        let start_time: Option<i64> = fields.get(19).and_then(|s| {
            let ticks: u64 = s.parse().ok()?;
            Some((boot_ms + (ticks * 1000 / hz)) as i64)
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
            host: host.clone(),
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

fn normalize_process_tree(procs: &mut Vec<ProcessEntry>) {
    let mut seen = HashSet::new();
    procs.retain(|p| p.process_id > 0 && seen.insert(p.process_id));

    let ids = procs.iter().map(|p| p.process_id).collect::<HashSet<_>>();
    for p in procs.iter_mut() {
        if let Some(ppid) = p.parent_process_id
            && (ppid <= 0 || ppid == p.process_id || !ids.contains(&ppid))
        {
            p.parent_process_id = None;
        }
    }

    let parent_by_pid = procs
        .iter()
        .filter_map(|p| p.parent_process_id.map(|ppid| (p.process_id, ppid)))
        .collect::<HashMap<_, _>>();
    for p in procs.iter_mut() {
        let mut ancestors = HashSet::new();
        ancestors.insert(p.process_id);
        let mut cursor = p.process_id;
        while let Some(parent) = parent_by_pid.get(&cursor).copied() {
            if !ancestors.insert(parent) {
                p.parent_process_id = None;
                break;
            }
            cursor = parent;
        }
    }
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
    let host = process_host();

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
            host: host.clone(),
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
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Win32Process {
    process_id: Option<i64>,
    parent_process_id: Option<i64>,
    name: Option<String>,
    executable_path: Option<String>,
    command_line: Option<String>,
    user: Option<String>,
}

#[cfg(windows)]
fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(windows)]
fn process_name(
    name: Option<String>,
    bin_path: Option<&String>,
    command_line: Option<&String>,
) -> String {
    if let Some(name) = non_empty(name) {
        return name;
    }
    if let Some(path) = bin_path {
        let normalized = path.replace('\\', "/");
        if let Some(last) = normalized.rsplit('/').next() {
            if !last.trim().is_empty() {
                return last.trim().to_string();
            }
        }
    }
    if let Some(command_line) = command_line {
        let first = command_line
            .trim()
            .trim_matches('"')
            .split_whitespace()
            .next()
            .unwrap_or("");
        let normalized = first.replace('\\', "/");
        if let Some(last) = normalized.rsplit('/').next() {
            if !last.trim().is_empty() {
                return last.trim().to_string();
            }
        }
    }
    "unknown".into()
}

#[cfg(windows)]
fn parse_powershell_process_json(stdout: &str) -> Result<Vec<ProcessEntry>, String> {
    let value: Value =
        serde_json::from_str(stdout.trim()).map_err(|e| format!("parse process json: {e}"))?;
    let values = match value {
        Value::Array(values) => values,
        Value::Object(_) => vec![value],
        Value::Null => Vec::new(),
        _ => return Err("unexpected process json shape".into()),
    };

    let mut out = Vec::new();
    let host = process_host();
    for value in values {
        let proc: Win32Process =
            serde_json::from_value(value).map_err(|e| format!("parse process entry: {e}"))?;
        let Some(pid) = proc.process_id else {
            continue;
        };
        let bin_path = non_empty(proc.executable_path);
        let command_line = non_empty(proc.command_line);
        let user = non_empty(proc.user);
        let name = process_name(proc.name, bin_path.as_ref(), command_line.as_ref());

        out.push(ProcessEntry {
            process_id: pid,
            name,
            host: host.clone(),
            parent_process_id: proc.parent_process_id,
            architecture: Some(std::env::consts::ARCH.into()),
            bin_path,
            command_line,
            user,
            ..Default::default()
        });
    }
    Ok(out)
}

#[cfg(windows)]
fn list_processes() -> Result<Vec<ProcessEntry>, String> {
    let ps_script = "Get-CimInstance Win32_Process | ForEach-Object { $owner = $_ | Invoke-CimMethod -MethodName GetOwner -ErrorAction SilentlyContinue; [pscustomobject]@{ ProcessId=$_.ProcessId; ParentProcessId=$_.ParentProcessId; Name=$_.Name; ExecutablePath=$_.ExecutablePath; CommandLine=$_.CommandLine; User=$(if ($owner.User) { if ($owner.Domain) { \"$($owner.Domain)\\\\$($owner.User)\" } else { $owner.User } } else { '' }) } } | ConvertTo-Json -Compress";
    if let Ok(output) = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            ps_script,
        ])
        .output()
    {
        let stdout = decode_output(&output.stdout);
        if output.status.success() && !stdout.trim().is_empty() {
            if let Ok(processes) = parse_powershell_process_json(&stdout) {
                if !processes.is_empty() {
                    return Ok(processes);
                }
            }
        }
    }

    let output = std::process::Command::new("wmic")
        .args([
            "process",
            "get",
            "ExecutablePath,Name,ParentProcessId,ProcessId",
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
    let host = process_host();

    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // CSV format is normally Node,ExecutablePath,Name,ParentProcessId,ProcessId.
        // Avoid CommandLine here because commas in arguments break wmic CSV parsing.
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 5 {
            continue;
        }

        let pid: i64 = match parts[4].trim_matches('"').parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let ppid: Option<i64> = parts[3].trim_matches('"').parse().ok();
        let bin_path = non_empty(Some(parts[1].trim_matches('"').to_string()));
        let name = process_name(
            Some(parts[2].trim_matches('"').to_string()),
            bin_path.as_ref(),
            None,
        );

        procs.push(ProcessEntry {
            process_id: pid,
            name,
            host: host.clone(),
            parent_process_id: ppid,
            architecture: Some(std::env::consts::ARCH.into()),
            bin_path,
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
    // `ps` accepts no parameters. We still validate that the parameter
    // string parses as an empty object — an operator who types
    // `ps -foo` will get a structured parameter-error response here
    // rather than a successful empty process list.
    let parameters = task.parameters.trim();
    let parameters = if parameters.is_empty() {
        "{}"
    } else {
        parameters
    };
    if let Err(e) = serde_json::from_str::<Params>(parameters) {
        return TaskResponse::failed(task.id, &format!("ps parse error: {e}"));
    }

    match list_processes() {
        Ok(mut procs) => {
            // Mythic's Process Browser groups rows by host; although the docs
            // say host is optional, the Rust wire struct serializes it as a
            // string. An empty string makes the browser associate the rows
            // with an empty host instead of the callback's host.
            //
            // Process Browser refreshes are full snapshots for this host and
            // callback tree group. Mythic only runs that refresh path when at
            // least one entry has update_deleted=true.
            for p in &mut procs {
                if p.host.trim().is_empty() {
                    p.host = process_host();
                }
                p.update_deleted = true;
            }
            normalize_process_tree(&mut procs);

            // BrowserScript receives normal response text, not only the
            // structured `processes` hook. Mirror Apollo by putting the
            // serialized process array in user_output while also sending
            // the hook data for Mythic's process browser.
            let user_output = serde_json::to_string(&procs).unwrap_or_else(|_| "[]".into());
            TaskResponse {
                task_id: task.id,
                completed: Some(true),
                status: Some("completed".into()),
                user_output: Some(user_output),
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
        assert!(resp.user_output.as_deref().unwrap_or("").starts_with('['));
        // Verify at least our own process appears
        let our_pid = std::process::id() as i64;
        assert!(resp.processes.iter().any(|p| p.process_id == our_pid));
    }

    #[test]
    fn test_ps_accepts_empty_parameters() {
        let task = TaskMessage {
            command: "ps".into(),
            parameters: "".into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
        assert!(!resp.processes.is_empty());
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
            assert!(!entry.host.is_empty());
        }
    }

    #[test]
    fn test_ps_marks_full_snapshot_for_process_browser() {
        let task = TaskMessage {
            command: "ps".into(),
            parameters: "{}".into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert!(!resp.processes.is_empty());
        assert!(resp.processes.iter().all(|p| p.update_deleted));
    }

    #[test]
    fn test_normalize_process_tree_removes_bad_edges() {
        let mut procs = vec![
            ProcessEntry {
                process_id: 1,
                name: "init".into(),
                parent_process_id: Some(0),
                ..Default::default()
            },
            ProcessEntry {
                process_id: 2,
                name: "self-parent".into(),
                parent_process_id: Some(2),
                ..Default::default()
            },
            ProcessEntry {
                process_id: 2,
                name: "duplicate".into(),
                parent_process_id: Some(1),
                ..Default::default()
            },
            ProcessEntry {
                process_id: 3,
                name: "missing-parent".into(),
                parent_process_id: Some(9999),
                ..Default::default()
            },
        ];

        normalize_process_tree(&mut procs);

        assert_eq!(procs.len(), 3);
        assert_eq!(procs[0].parent_process_id, None);
        assert_eq!(procs[1].parent_process_id, None);
        assert_eq!(procs[2].parent_process_id, None);
    }

    #[test]
    fn test_normalize_process_tree_breaks_cycles() {
        let mut procs = vec![
            ProcessEntry {
                process_id: 10,
                name: "a".into(),
                parent_process_id: Some(11),
                ..Default::default()
            },
            ProcessEntry {
                process_id: 11,
                name: "b".into(),
                parent_process_id: Some(12),
                ..Default::default()
            },
            ProcessEntry {
                process_id: 12,
                name: "c".into(),
                parent_process_id: Some(10),
                ..Default::default()
            },
        ];

        normalize_process_tree(&mut procs);

        assert!(procs.iter().any(|p| p.parent_process_id.is_none()));
    }
}
