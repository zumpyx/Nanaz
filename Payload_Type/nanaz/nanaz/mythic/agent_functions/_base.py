"""Shared base class + helpers for nanaz agent_functions modules.

Reduces boilerplate across file-browser-style commands (ls, download, upload,
rm, cat, cp, mv, mkdir) that share the same "Mythic UI sends
{host, path, full_path}; CLI sends a single string" parsing logic.

This is a refactor, not a behaviour change — the per-command classes still
define their own CommandParameter list and create_go_tasking. They just
inherit the parse_dictionary / parse_arguments boilerplate.
"""
import json
from typing import Any, Dict

from mythic_container.MythicCommandBase import (
    CommandBase,
    CommandAttributes,
    CommandParameter,
    ParameterType,
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

        Prefer full_path (matches what the user clicked) and pass host through
        only when non-empty.
        """
        if "host" in dictionary_arguments and dictionary_arguments.get("full_path"):
            self.set_arg("path", dictionary_arguments["full_path"])
            if dictionary_arguments.get("host"):
                self.set_arg("host", dictionary_arguments["host"])
            return
        self.load_args_from_dictionary(dictionary_arguments)

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
                if data.get("full_path"):
                    self.set_arg("path", data["full_path"])
                    if data.get("host"):
                        self.set_arg("host", data["host"])
                    return
                if "path" in data:
                    self.set_arg("path", data["path"])
                    return
            except json.JSONDecodeError:
                pass
        if self.cli_takes_path:
            self.set_arg("path", cl)


def simple_command_attributes(supported_os=None, builtin: bool = False) -> CommandAttributes:
    """Default attributes used by every nanaz command."""
    if supported_os is None:
        supported_os = [SupportedOS.Windows, SupportedOS.Linux]
    return CommandAttributes(
        spawn_and_injectable=False,
        supported_os=supported_os,
        builtin=builtin,
        load_only=False,
        suggested_command=False,
    )
