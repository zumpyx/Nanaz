"""rm — remove files / directories on the target.

Cross-platform. The Rust side enforces two safety guards:

  1. System paths (e.g. /etc, C:\\Windows) are refused unless the
     operator opts in with `allow_system_path: true`.
  2. Recursive deletion of directories requires
     `confirm_destructive: true` regardless of the path.

The Mythic file browser's "Delete" action (`file_browser:remove`)
triggers an `rm` task but doesn't expose a CLI form, so it has no
way to set the `confirm_destructive` flag. The browser already pops
its own confirmation dialog (the user clicks "Delete" in a modal),
so we inject `confirm_destructive: true` whenever the request comes
from the file browser. CLI invocations still have to type the flag
explicitly.
"""

from mythic_container.MythicCommandBase import *

from ._base import (
    FileBrowserArguments,
    error_aware_process_response,
    simple_command_attributes,
    split_cli_preserve_backslashes,
)


class RmArguments(FileBrowserArguments):
    cli_takes_path = True
    command_name = "rm"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(name="path", type=ParameterType.String, default_value=""),
            CommandParameter(name="recursive", type=ParameterType.Boolean, default_value=False),
            CommandParameter(
                name="confirm_destructive",
                type=ParameterType.Boolean,
                default_value=False,
                description=(
                    "Required for recursive removal. The Mythic file browser "
                    "injects this automatically when an operator confirms a "
                    "delete; CLI users must type `-r --confirm-destructive`."
                ),
            ),
        ]

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if not cl or cl.startswith("{"):
            await super().parse_arguments()
            return
        # Tokenise: `-r` / `-rf` / `-R` and `--confirm-destructive`.
        # `-rf` and `-fr` are POSIX shorthand for "recursive + force"
        # (i.e. set both flags in one go); without that, operators
        # who type the natural `rm -rf foo` would have to type the
        # long flags separately, which defeats the purpose of the
        # shorthand.
        try:
            parts = split_cli_preserve_backslashes(cl)
        except ValueError as e:
            raise Exception(f"rm: failed to parse command line: {e}")

        recursive = False
        confirm = False
        path_parts = []
        for tok in parts:
            low = tok.lower()
            if low in ("-r", "--recursive"):
                recursive = True
            elif low in ("-rf", "-fr"):
                recursive = True
                confirm = True
            elif low in ("--confirm-destructive", "--confirm", "-y", "--yes", "-f", "/f"):
                confirm = True
            elif low.startswith("-"):
                # Unknown flag — let the agent side fail loudly rather
                # than silently ignore.
                raise Exception(f"rm: unknown flag {tok}")
            else:
                path_parts.append(tok)
        if not path_parts:
            raise Exception("rm: missing path argument")
        self.set_arg("path", " ".join(path_parts))
        if recursive:
            self.set_arg("recursive", True)
        if confirm:
            self.set_arg("confirm_destructive", True)

    async def parse_dictionary(self, dictionary_arguments):
        """Mythic UI file-browser sends {host, full_path, path, file}.

        Two UI quirks to handle:

        1. The browser does not pass `confirm_destructive`. Recursive
           deletes triggered by clicking the "X" on a folder would
           otherwise fail with "requires confirm_destructive=true".
           Inject it on the assumption that the user already clicked
           through the file browser's confirmation modal.

        2. The browser may pass `recursive` as a UI hint; honour it.
        """
        await super().parse_dictionary(dictionary_arguments)
        # File-browser delete = user has confirmed in UI. Auto-inject
        # the destructive confirmation so the Rust side accepts the op.
        if not self.get_arg("confirm_destructive"):
            self.set_arg("confirm_destructive", True)


class RmCommand(CommandBase):
    cmd = "rm"
    needs_admin = False
    help_cmd = "rm [path] [-r] [--confirm-destructive]"
    description = (
        "Remove a file or directory (use -r for directories). "
        "Recursive removal requires --confirm-destructive."
    )
    version = 2
    author = "@zumpyx"
    argument_class = RmArguments
    attackmapping = ["T1070"]
    supported_ui_features = ["file_browser:remove"]
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        path = taskData.args.get_arg("path")
        rec = taskData.args.get_arg("recursive")
        response.DisplayParams = f"{path}" + (" -r" if rec else "")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
