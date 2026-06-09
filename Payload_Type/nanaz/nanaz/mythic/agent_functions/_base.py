"""Shared base class + helpers for nanaz agent_functions modules.

Reduces boilerplate across file-browser-style commands (ls, download, upload,
rm, cat, cp, mv, mkdir) that share the same "Mythic UI sends
{host, path, full_path}; CLI sends a single string" parsing logic.

This is a refactor, not a behaviour change — the per-command classes still
define their own CommandParameter list and create_go_tasking. They just
inherit the parse_dictionary / parse_arguments boilerplate.
"""
import json
import shlex
from typing import Any, Dict

from mythic_container.MythicCommandBase import (
    CommandBase,
    CommandAttributes,
    CommandParameter,
    ParameterType,
    PTTaskMessageAllData,
    PTTaskProcessResponseMessageResponse,
    SupportedOS,
    TaskArguments,
)


class FileBrowserArguments(TaskArguments):
    """Base argument class for commands that accept a path / host / full_path.

    Subclasses must set `cli_takes_path` to True if the CLI form is a bare
    path string, and override `command_name` for nicer error messages.
    """

    cli_takes_path: bool = True
    command_name: str = "<command>"

    async def parse_dictionary(self, dictionary_arguments: Dict[str, Any]) -> None:
        """Mythic UI file-browser sends {host, path, file, full_path}.

        Prefer full_path (matches what the user clicked). `host` is UI
        metadata, not an agent parameter for nanaz's Rust commands: unlike
        Apollo's C# agent, nanaz does not reconstruct remote paths from
        {host, path}. Keep the clicked path intact and do not add host to the
        task payload.
        """
        arg_names = {arg.name for arg in self.args}
        clean_args = {
            k: v
            for k, v in dictionary_arguments.items()
            if k in arg_names and k not in ("host", "full_path")
        }

        if "path" in arg_names:
            clean_args["path"] = browser_full_path(dictionary_arguments)

        self.load_args_from_dictionary(clean_args)
        if dictionary_arguments.get("host"):
            self.add_arg("host", dictionary_arguments["host"])

    async def parse_arguments(self) -> None:
        """CLI form: either a single path string, or a JSON object."""
        cl = self.command_line.strip()
        if not cl:
            # Some commands (ls, rm) accept the parameter from the file
            # browser, not the CLI. Let Mythic fill in defaults.
            return
        if cl.startswith("{"):
            try:
                data = json.loads(cl)
                arg_names = {arg.name for arg in self.args}
                clean_args = {
                    k: v
                    for k, v in data.items()
                    if k in arg_names and k not in ("host", "full_path")
                }
                if "path" in arg_names:
                    clean_args["path"] = browser_full_path(data)
                self.load_args_from_dictionary(clean_args)
                if data.get("host"):
                    self.add_arg("host", data["host"])
                return
            except json.JSONDecodeError:
                pass
        if self.cli_takes_path:
            self.set_arg("path", _strip_outer_quotes(cl))


def simple_command_attributes(
    supported_os=None,
    builtin: bool = False,
    suggested_command: bool = False,
) -> CommandAttributes:
    """Default attributes used by every nanaz command."""
    if supported_os is None:
        supported_os = [SupportedOS.Windows, SupportedOS.Linux]
    return CommandAttributes(
        spawn_and_injectable=False,
        supported_os=supported_os,
        builtin=builtin,
        load_only=False,
        suggested_command=suggested_command,
    )


def split_cli_preserve_backslashes(command_line: str) -> list[str]:
    """Split CLI text without treating Windows backslashes as escapes."""
    lexer = shlex.shlex(command_line, posix=False)
    lexer.whitespace_split = True
    lexer.commenters = ""
    return [_strip_outer_quotes(token) for token in lexer]


def read_cli_token(command_line: str, offset: int = 0) -> tuple[str, int, int]:
    """Read one shell-like token, preserving backslashes and returning spans."""
    i = offset
    n = len(command_line)
    while i < n and command_line[i].isspace():
        i += 1
    start = i
    quote = None
    token = []
    while i < n:
        ch = command_line[i]
        if quote:
            if ch == quote:
                quote = None
            else:
                token.append(ch)
            i += 1
            continue
        if ch in ("'", '"'):
            quote = ch
            i += 1
            continue
        if ch.isspace():
            break
        token.append(ch)
        i += 1
    return "".join(token), start, i


def _strip_outer_quotes(token: str) -> str:
    if len(token) >= 2 and token[0] == token[-1] and token[0] in ("'", '"'):
        return token[1:-1]
    return token


def browser_full_path(arguments: Dict[str, Any]) -> str:
    """Normalize Mythic file-browser tasking into the agent's path argument."""
    full_path = arguments.get("full_path")
    if full_path:
        return normalize_browser_path(str(full_path))

    parent = arguments.get("path")
    name = arguments.get("file")
    if parent is not None and name:
        parent = str(parent)
        name = str(name)
        if not parent:
            return normalize_browser_path(name)
        if name.startswith(("/", "\\")) or (len(name) >= 2 and name[1] == ":"):
            return normalize_browser_path(name)
        sep = "\\" if "\\" in parent or (len(parent) >= 2 and parent[1] == ":") else "/"
        if parent.endswith(("/", "\\")):
            return normalize_browser_path(parent + name)
        return normalize_browser_path(parent + sep + name)

    if parent is not None:
        return normalize_browser_path(str(parent))
    if name is not None:
        return normalize_browser_path(str(name))
    return ""


def normalize_browser_path(path: str) -> str:
    """Keep Windows drive roots as roots when Mythic sends `C:`."""
    if len(path) == 2 and path[1] == ":" and path[0].isalpha():
        return path + "\\"
    return path


def error_aware_process_response(
    task: PTTaskMessageAllData,
    response: Any,
) -> PTTaskProcessResponseMessageResponse:
    """Default process_response that surfaces Rust-side errors.

    Without this, every command's process_response returned
    `PTTaskProcessResponseMessageResponse(Success=True)` regardless of
    whether the Rust agent reported `status: "error"`. The Mythic UI would
    mark such tasks as successful even when the operator saw an error string
    in stdout. This helper inspects the response payload and propagates
    the error so the UI marks the task as failed.
    """
    resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
    if isinstance(response, dict):
        status = response.get("status")
        if status == "error":
            resp.Success = False
            user_output = response.get("user_output") or "agent reported an error"
            resp.Error = user_output
    return resp


def validate_timeout(timeout: int | None, max_seconds: int = 3600) -> str | None:
    if timeout is None:
        return None
    if timeout < 1 or timeout > max_seconds:
        return f"timeout must be between 1 and {max_seconds} seconds."
    return None
