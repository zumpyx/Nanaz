"""pwd — print the agent's current working directory.

Cross-platform. The Rust agent reports its process cwd and includes it
in `process_response.cwd`; this wrapper mirrors that value into Mythic's
persistent callback state so the file browser and command output stay
consistent.
"""

from mythic_container.MythicCommandBase import (
    CommandBase,
    CommandAttributes,
    PTTaskCreateTaskingMessageResponse,
    PTTaskMessageAllData,
    PTTaskProcessResponseMessageResponse,
    SupportedOS,
    TaskArguments,
)
from mythic_container.MythicGoRPC.send_mythic_rpc_callback_update import (
    MythicRPCCallbackUpdateMessage,
    SendMythicRPCCallbackUpdate,
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
        suggested_command=True,
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
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        if isinstance(response, dict):
            if response.get("status") == "error":
                resp.Success = False
                resp.Error = response.get("user_output") or "pwd failed"
                return resp
            cwd = response.get("cwd")
            if not cwd:
                pr = response.get("process_response")
                if isinstance(pr, dict):
                    cwd = pr.get("cwd")
            if cwd:
                await SendMythicRPCCallbackUpdate(
                    MythicRPCCallbackUpdateMessage(
                        CallbackID=task.Callback.ID,
                        Cwd=str(cwd),
                    )
                )
        return resp
