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

        if dictionary_arguments.get("full_path") and "path" in arg_names:
            clean_args["path"] = dictionary_arguments["full_path"]
        elif dictionary_arguments.get("path") is not None and "path" in arg_names:
            clean_args["path"] = dictionary_arguments["path"]
        elif (
            dictionary_arguments.get("file") is not None
            and "path" in arg_names
            and "file" not in arg_names
        ):
            clean_args["path"] = dictionary_arguments["file"]

        self.load_args_from_dictionary(clean_args)

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
                if data.get("full_path") and "path" in arg_names:
                    clean_args["path"] = data["full_path"]
                elif "path" in data and "path" in arg_names:
                    clean_args["path"] = data["path"]
                elif "file" in data and "path" in arg_names and "file" not in arg_names:
                    clean_args["path"] = data["file"]
                self.load_args_from_dictionary(clean_args)
                return
            except json.JSONDecodeError:
                pass
        if self.cli_takes_path:
            self.set_arg("path", cl)


def simple_command_attributes(
    supported_os=None,
    builtin: bool = False,
    suggested_command: bool = True,
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


def _strip_outer_quotes(token: str) -> str:
    if len(token) >= 2 and token[0] == token[-1] and token[0] in ("'", '"'):
        return token[1:-1]
    return token


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
