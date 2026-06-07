//! Fallback help command.
//!
//! Mythic normally handles `help` server-side, but keeping an agent-side
//! fallback prevents an empty/unknown result if a callback tasks it directly.

use mythic::{TaskMessage, TaskResponse};
use serde::Deserialize;

#[derive(Deserialize, Default)]
struct Params {
    #[serde(default)]
    command: Option<String>,
}

const COMMANDS: &[(&str, &str, &str)] = &[
    ("bash", "bash [command]", "Run a Bash command."),
    ("cat", "cat [path]", "Read and display file contents."),
    ("cd", "cd [path]", "Change the current working directory."),
    ("cmd", "cmd [command]", "Run a Windows cmd.exe command."),
    ("cp", "cp [src] [dst]", "Copy a file."),
    (
        "download",
        "download [path]",
        "Download a file from the target.",
    ),
    (
        "drives",
        "drives",
        "List available filesystem roots / drives.",
    ),
    ("env", "env [filter_key]", "List environment variables."),
    (
        "execute",
        "execute [path] [arguments]",
        "Execute a process.",
    ),
    (
        "execute_assembly",
        "execute_assembly [Assembly.exe] [args]",
        "Execute a .NET assembly.",
    ),
    ("exit", "exit [process]", "Exit the agent or callback."),
    ("help", "help [command]", "Show command help."),
    ("kill", "kill <pid> [-9]", "Kill a process."),
    ("ls", "ls [path]", "List files and directories."),
    ("mkdir", "mkdir [path]", "Create a directory."),
    ("mv", "mv [src] [dst]", "Move or rename a file."),
    ("netstat", "netstat", "List network connections."),
    (
        "powerpick",
        "powerpick [command]",
        "Run PowerShell through CLR hosting.",
    ),
    (
        "powershell",
        "powershell [command]",
        "Run a PowerShell command.",
    ),
    ("ps", "ps", "List processes for Mythic's process browser."),
    (
        "pty",
        "pty [sh|bash|cmd|powershell]",
        "Start an interactive shell task.",
    ),
    ("pwd", "pwd", "Print the current working directory."),
    ("resolve", "resolve [hostname]", "Resolve a hostname."),
    (
        "rm",
        "rm [path] [-r] [--confirm-destructive]",
        "Remove a file or directory.",
    ),
    (
        "rpfwd",
        "rpfwd -Port [port] -RemoteIP [ip] -RemotePort [port]",
        "Start or stop a reverse port forward.",
    ),
    ("sh", "sh [command]", "Run a POSIX shell command."),
    (
        "sleep",
        "sleep [seconds] [jitter]",
        "Set callback sleep and jitter.",
    ),
    (
        "socks",
        "socks -Port [port] -Action start",
        "Start or stop a SOCKS5 listener.",
    ),
    ("sysinfo", "sysinfo", "Gather system information."),
    ("tree", "tree [path]", "Recursively list a directory tree."),
    (
        "upload",
        "upload [destination_path]",
        "Upload a file to the target.",
    ),
    (
        "wget",
        "wget [url] [destination_path]",
        "Download a URL to disk.",
    ),
    ("whoami", "whoami", "Print the current user."),
];

fn parse_params(task: &TaskMessage) -> Result<Params, String> {
    let parameters = task.parameters.trim();
    let parameters = if parameters.is_empty() {
        "{}"
    } else {
        parameters
    };
    serde_json::from_str::<Params>(parameters).map_err(|e| format!("help parse error: {e}"))
}

fn all_help() -> String {
    let mut lines = Vec::with_capacity(COMMANDS.len() + 1);
    lines.push("Available commands:".to_string());
    for (name, usage, description) in COMMANDS {
        lines.push(format!("  {name:<16} {usage:<42} {description}"));
    }
    lines.join("\n")
}

fn command_help(name: &str) -> String {
    let wanted = name.trim().to_lowercase();
    for (command, usage, description) in COMMANDS {
        if *command == wanted {
            return format!("{command}\nUsage: {usage}\n{description}");
        }
    }
    format!("unknown command: {name}\n\n{}", all_help())
}

pub fn handle(task: &TaskMessage) -> TaskResponse {
    let params = match parse_params(task) {
        Ok(params) => params,
        Err(e) => return TaskResponse::failed(task.id, &e),
    };
    let output = params
        .command
        .as_deref()
        .filter(|command| !command.trim().is_empty())
        .map(command_help)
        .unwrap_or_else(all_help);

    TaskResponse {
        task_id: task.id,
        completed: Some(true),
        status: Some("completed".into()),
        user_output: Some(output),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_lists_commands() {
        let task = TaskMessage {
            command: "help".into(),
            parameters: "".into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
        let output = resp.user_output.unwrap_or_default();
        assert!(output.contains("ls [path]"));
        assert!(output.contains("drives"));
    }

    #[test]
    fn test_help_specific_command() {
        let task = TaskMessage {
            command: "help".into(),
            parameters: r#"{"command":"ls"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
        assert!(
            resp.user_output
                .unwrap_or_default()
                .contains("Usage: ls [path]")
        );
    }
}
