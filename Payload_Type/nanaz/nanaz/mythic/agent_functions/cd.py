"""cd — change the agent's current working directory.

Cross-platform. The Rust agent validates the path, updates the OS
process's cwd, and emits the new value via the structured `cwd` field
on the response. This Python wrapper mirrors that value into
Mythic's persistent callback state via `callback_update` (Cwd) so
the file browser UI's "current location" line and the next `ls` call
stay in sync.

`cd` does not need to be wrapped with `FileBrowserArguments` because
the Mythic file browser never calls it — the browser navigates by
issuing `ls` on a new path. CLI / typed `cd` goes through
`parse_arguments`.
"""

from mythic_container.MythicCommandBase import (
    CommandBase,
    CommandAttributes,
    CommandParameter,
    ParameterType,
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


class CdArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="path",
                type=ParameterType.String,
                description="Directory to change into.",
            ),
            CommandParameter(
                name="allow_system_path",
                type=ParameterType.Boolean,
                default_value=False,
                description="Allow cd into protected system paths (default false).",
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        path = self.command_line.strip()
        if not path:
            raise Exception("cd requires a path")
        self.set_arg("path", path.strip("\"'"))


class CdCommand(CommandBase):
    cmd = "cd"
    needs_admin = False
    help_cmd = "cd [path]"
    description = (
        "Change the agent's current working directory. The new value "
        "is mirrored to Mythic's callback state so the file browser "
        "shows it on the next refresh."
    )
    version = 1
    author = "@zumpyx"
    argument_class = CdArguments
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
        path = taskData.args.get_arg("path") or ""
        return PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
            DisplayParams=path,
        )

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        """Mirror the new cwd into Mythic's persistent callback state.

        Mythic ships the cwd in two places: the task output (the
        agent's user_output) and the callback's `cwd` field. The
        latter is what the file browser reads, so we push the update
        explicitly via `callback_update`. The Mythic RPCCallbackUpdate
        interface accepts AgentCallbackID or CallbackID; we use the
        integer ID (the .id of the callback object) for the most
        stable lookup.

        The Rust side returns the cwd inside the structured
        `process_response` JSON (Mythic's wire protocol doesn't have
        a dedicated cwd field on TaskResponse), so we dig it out
        of that.
        """
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        cwd = None
        if isinstance(response, dict):
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
