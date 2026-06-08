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

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandOs {
    All,
    Linux,
    Windows,
}

struct CommandHelp {
    name: &'static str,
    usage: &'static str,
    description: &'static str,
    os: CommandOs,
}

impl CommandHelp {
    fn supported_on_current_os(&self) -> bool {
        match self.os {
            CommandOs::All => true,
            CommandOs::Linux => cfg!(target_os = "linux"),
            CommandOs::Windows => cfg!(windows),
        }
    }
}

const COMMANDS: &[CommandHelp] = &[
    CommandHelp {
        name: "bash",
        usage: "bash [command]",
        description: "Run a Bash command.",
        os: CommandOs::Linux,
    },
    CommandHelp {
        name: "cat",
        usage: "cat [path]",
        description: "Read and display file contents.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "cd",
        usage: "cd [path]",
        description: "Change the current working directory.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "cmd",
        usage: "cmd [command]",
        description: "Run a Windows cmd.exe command.",
        os: CommandOs::Windows,
    },
    CommandHelp {
        name: "cp",
        usage: "cp [src] [dst]",
        description: "Copy a file.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "download",
        usage: "download [path]",
        description: "Download a file from the target.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "drives",
        usage: "drives",
        description: "List available filesystem roots / drives.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "env",
        usage: "env [filter_key]",
        description: "List environment variables.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "execute",
        usage: "execute [path] [arguments]",
        description: "Execute a process.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "execute_assembly",
        usage: "execute_assembly [Assembly.exe] [args]",
        description: "Execute a .NET assembly.",
        os: CommandOs::Windows,
    },
    CommandHelp {
        name: "exit",
        usage: "exit [process]",
        description: "Exit the agent or callback.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "help",
        usage: "help [command]",
        description: "Show command help.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "kill",
        usage: "kill <pid> [-9]",
        description: "Kill a process.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "ls",
        usage: "ls [path]",
        description: "List files and directories.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "mkdir",
        usage: "mkdir [path]",
        description: "Create a directory.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "mv",
        usage: "mv [src] [dst]",
        description: "Move or rename a file.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "netstat",
        usage: "netstat",
        description: "List network connections.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "powerpick",
        usage: "powerpick [command]",
        description: "Run PowerShell through CLR hosting.",
        os: CommandOs::Windows,
    },
    CommandHelp {
        name: "powershell",
        usage: "powershell [command]",
        description: "Run a PowerShell command.",
        os: CommandOs::Windows,
    },
    CommandHelp {
        name: "ps",
        usage: "ps",
        description: "List processes for Mythic's process browser.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "pty",
        usage: "pty [sh|bash|cmd|powershell]",
        description: "Start an interactive shell task.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "pwd",
        usage: "pwd",
        description: "Print the current working directory.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "resolve",
        usage: "resolve [hostname]",
        description: "Resolve a hostname.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "rm",
        usage: "rm [path] [-r] [--confirm-destructive]",
        description: "Remove a file or directory.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "rpfwd",
        usage: "rpfwd -Port [port] -RemoteIP [ip] -RemotePort [port]",
        description: "Start or stop a reverse port forward.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "sh",
        usage: "sh [command]",
        description: "Run a POSIX shell command.",
        os: CommandOs::Linux,
    },
    CommandHelp {
        name: "sleep",
        usage: "sleep [seconds] [jitter]",
        description: "Set callback sleep and jitter.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "socks",
        usage: "socks -Port [port] -Action start",
        description: "Start or stop a SOCKS5 listener.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "sysinfo",
        usage: "sysinfo",
        description: "Gather system information.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "tree",
        usage: "tree [path]",
        description: "Recursively list a directory tree.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "upload",
        usage: "upload [destination_path]",
        description: "Upload a file to the target.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "wget",
        usage: "wget [url] [destination_path]",
        description: "Download a URL to disk.",
        os: CommandOs::All,
    },
    CommandHelp {
        name: "whoami",
        usage: "whoami",
        description: "Print the current user.",
        os: CommandOs::All,
    },
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
    for command in COMMANDS
        .iter()
        .filter(|command| command.supported_on_current_os())
    {
        lines.push(format!(
            "  {:<16} {:<42} {}",
            command.name, command.usage, command.description
        ));
    }
    lines.join("\n")
}

fn command_help(name: &str) -> String {
    let wanted = name.trim().to_lowercase();
    for command in COMMANDS {
        if command.name == wanted {
            if !command.supported_on_current_os() {
                return format!("{} is not supported on this operating system", command.name);
            }
            return format!(
                "{}\nUsage: {}\n{}",
                command.name, command.usage, command.description
            );
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
        #[cfg(target_os = "linux")]
        {
            assert!(output.contains("bash [command]"));
            assert!(output.contains("sh [command]"));
            assert!(!output.contains("cmd [command]"));
            assert!(!output.contains("powershell [command]"));
            assert!(!output.contains("execute_assembly [Assembly.exe]"));
        }
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

    #[cfg(target_os = "linux")]
    #[test]
    fn test_help_rejects_unsupported_command() {
        let task = TaskMessage {
            command: "help".into(),
            parameters: r#"{"command":"cmd"}"#.into(),
            ..Default::default()
        };
        let resp = handle(&task);
        assert_eq!(resp.status.as_deref(), Some("completed"));
        assert!(
            resp.user_output
                .unwrap_or_default()
                .contains("cmd is not supported on this operating system")
        );
    }
}
