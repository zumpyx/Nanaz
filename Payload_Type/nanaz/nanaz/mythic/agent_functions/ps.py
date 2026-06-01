from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
from mythic_container.MythicGoRPC.send_mythic_rpc_task_update import (
    MythicRPCTaskUpdateMessage,
    SendMythicRPCTaskUpdate,
)


class PsArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(name="host", type=ParameterType.String, default_value=""),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        pass


class PsCommand(CommandBase):
    cmd = "ps"
    needs_admin = False
    help_cmd = "ps"
    description = "List running processes. Integrates with Mythic's process browser UI."
    version = 1
    author = "@zumpyx"
    argument_class = PsArguments
    attackmapping = ["T1057"]
    supported_ui_features = ["process_browser:list"]
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
        response = PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )
        response.DisplayParams = "list processes"
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        # Format processes structured data into human-readable output
        if isinstance(response, dict) and "processes" in response:
            procs = response["processes"]
            if procs:
                lines = [f"{'PID':<8} {'PPID':<8} {'NAME':<24} CMDLINE"]
                lines.append("-" * 72)
                for p in procs[:40]:
                    name = p.get("name", "")
                    if len(name) > 24:
                        name = name[:23] + "…"
                    cmd = p.get("command_line") or "-"
                    if len(cmd) > 40:
                        cmd = cmd[:39] + "…"
                    lines.append(
                        f"{p.get('process_id', 0):<8} "
                        f"{str(p.get('parent_process_id', '-')):<8} "
                        f"{name:<24} "
                        f"{cmd}"
                    )
                if len(procs) > 40:
                    lines.append(f"… and {len(procs) - 40} more")
                lines.append(f"── {len(procs)} processes ──")
                output = "\n".join(lines)
                await SendMythicRPCTaskUpdate(
                    MythicRPCTaskUpdateMessage(TaskID=task.Task.ID, UpdateStdout=output)
                )
        return resp
