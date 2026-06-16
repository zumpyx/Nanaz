"""kill — terminate a process by PID on the target.

Cross-platform. The Rust side picks the platform-appropriate syscall
(`kill(pid, SIGTERM)` on Unix, `TerminateProcess` on Windows); this
wrapper only formats the parameters and the response.

`kill` is also exposed as a process_browser action — the right-click
"kill" menu in the new-UI process list issues a `kill` task with
`{pid: <number>}`. The Mythic UI does not let the user set
`force: true` from the context menu (the menu's whole point is the
fast happy path); for that, the operator types `kill -9 <pid>` from
the CLI.
"""

import json

from mythic_container.MythicCommandBase import *

from ._base import (
    error_aware_process_response,
    simple_command_attributes,
)


class KillArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="pid",
                type=ParameterType.Number,
                description="PID of the process to terminate.",
            ),
            CommandParameter(
                name="force",
                type=ParameterType.Boolean,
                default_value=False,
                description=(
                    "On Unix, escalate SIGTERM to SIGKILL when the "
                    "polite signal fails (permission denied, etc.). "
                    "Ignored on Windows."
                ),
            ),
        ]

    async def parse_arguments(self):
        """CLI: `kill <pid>` or `kill -9 <pid>` / `--force <pid>`.

        The new-UI process browser sends a JSON dict ({pid: ...}) so
        the JSON path is the common case in practice; the CLI parsing
        only matters for operators typing the command directly.
        """
        cl = self.command_line.strip()
        if not cl:
            raise Exception("kill: missing pid; usage: kill <pid>")
        if cl.startswith("{"):
            try:
                data = json.loads(cl)
            except json.JSONDecodeError as e:
                raise Exception(f"kill: invalid JSON: {e}")
            pid = data.get("pid", data.get("process_id"))
            if pid is None:
                raise Exception("kill: JSON missing 'pid' or 'process_id' field")
            self.set_arg("pid", int(pid))
            if "force" in data:
                self.set_arg("force", bool(data["force"]))
            return

        # Freeform: tokenise and pick the first non-flag token as the
        # pid; -9 / -SIGKILL / --force set the force flag.
        force = False
        pid: int | None = None
        for tok in cl.split():
            low = tok.lower()
            if low in ("-9", "-sigkill", "--force", "-f", "/f"):
                force = True
            elif low.startswith("-"):
                # Unknown flag — pass through to the agent side so
                # operators get a clear "unknown flag" error from a
                # single place rather than silent acceptance here.
                raise Exception(f"kill: unknown flag {tok}")
            else:
                try:
                    pid = int(tok)
                except ValueError:
                    raise Exception(f"kill: not a valid pid: {tok!r}")
        if pid is None:
            raise Exception("kill: missing pid; usage: kill <pid>")
        if pid <= 0:
            raise Exception(f"kill: invalid pid {pid} (must be > 0)")
        self.set_arg("pid", pid)
        if force:
            self.set_arg("force", True)

    async def parse_dictionary(self, dictionary_arguments):
        """Process browser context menu sends {process_id: N}; custom tables may send {pid: N}."""
        pid = dictionary_arguments.get("pid", dictionary_arguments.get("process_id"))
        if pid is None:
            raise Exception("kill: missing 'pid' or 'process_id' in task parameters")
        self.set_arg("pid", int(pid))
        if "force" in dictionary_arguments:
            self.set_arg("force", bool(dictionary_arguments["force"]))


class KillCommand(CommandBase):
    cmd = "kill"
    needs_admin = False
    help_cmd = "kill <pid> [-9]"
    description = (
        "Terminate a process by PID. Sends SIGTERM (Unix) or "
        "TerminateProcess (Windows); pass -9 to escalate to SIGKILL "
        "when SIGTERM fails (Unix only)."
    )
    version = 1
    author = "@zumpyx"
    argument_class = KillArguments
    attackmapping = ["T1106", "T1562"]
    # `kill` is invoked by the new-UI process browser's right-click
    # "Kill" menu (ui_feature `process_browser:kill`). The bare
    # `kill` feature is so external / scripted callers can reference
    # it as a generic kill capability.
    supported_ui_features = ["kill", "process_browser:kill"]
    attributes = simple_command_attributes(suggested_command=True)

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        pid = taskData.args.get_arg("pid")
        force = taskData.args.get_arg("force")
        return PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
            DisplayParams=f"-{'9 ' if force else ''}PID {pid}",
        )

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
