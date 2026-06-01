"""pwd — print the agent's current working directory.

Cross-platform. Mythic tracks each callback's `current_working_directory`
in the callback's persistent state and surfaces it in the UI; this
command simply returns that value to the operator's task output.

The Rust agent keeps its own cwd in sync with Mythic's state (and emits
it on every command response), so `pwd` does not need a Rust-side
handler — it works even on a fresh callback by reading the
auto-initialised value from `TaskMessage.cwd`.
"""

from mythic_container.MythicCommandBase import (
    CommandBase,
    PTTaskCreateTaskingMessageResponse,
    PTTaskMessageAllData,
    PTTaskProcessResponseMessageResponse,
    TaskArguments,
)
from mythic_container.MythicGoRPC.send_mythic_rpc_task_update import (
    MythicRPCTaskUpdateMessage,
    SendMythicRPCTaskUpdate,
)


class PwdArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = []

    async def parse_arguments(self):
        # `pwd` takes no arguments. Anything typed is a usage error.
        if self.command_line.strip():
            raise Exception("pwd takes no arguments.")


class PwdCommand(CommandBase):
    cmd = "pwd"
    needs_admin = False
    help_cmd = "pwd"
    description = (
        "Print the agent's current working directory. Mirrors the Unix "
        "`pwd` builtin; the value is the same as Mythic's "
        "current_working_directory state for this callback."
    )
    version = 1
    author = "@zumpyx"
    argument_class = PwdArguments
    attackmapping = ["T1083"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=False,
    )

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        return PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
            DisplayParams="",
        )

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        """Format the structured cwd response as a one-line output.

        Mythic tracks the cwd in the callback's persistent state, but
        that value isn't automatically streamed to the task output —
        we explicitly write it so the operator sees a clean
        `pwd → /path/to/dir` line in the tasking panel.
        """
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        if isinstance(response, dict):
            cwd = response.get("cwd") or response.get("user_output")
            if cwd:
                await SendMythicRPCTaskUpdate(
                    MythicRPCTaskUpdateMessage(
                        TaskID=task.Task.ID,
                        UpdateStdout=str(cwd).rstrip(),
                    )
                )
        return resp
