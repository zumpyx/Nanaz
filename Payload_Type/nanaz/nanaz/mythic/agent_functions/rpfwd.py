import json
import shlex

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import (
    MythicRPCProxyStartMessage,
    MythicRPCProxyStopMessage,
    MythicRPCResponseCreateMessage,
    SendMythicRPCProxyStartCommand,
    SendMythicRPCProxyStopCommand,
    SendMythicRPCResponseCreate,
)


class RpfwdArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="port",
                cli_name="Port",
                display_name="Listen Port",
                type=ParameterType.Number,
                description="Port to listen on where the nanaz agent is running.",
                parameter_group_info=[ParameterGroupInfo(required=True, ui_position=0)],
            ),
            CommandParameter(
                name="remote_ip",
                cli_name="RemoteIP",
                display_name="Remote IP",
                type=ParameterType.String,
                description="Remote IP Mythic connects to for forwarded traffic.",
                parameter_group_info=[ParameterGroupInfo(required=True, ui_position=1)],
            ),
            CommandParameter(
                name="remote_port",
                cli_name="RemotePort",
                display_name="Remote Port",
                type=ParameterType.Number,
                description="Remote port Mythic connects to for forwarded traffic.",
                parameter_group_info=[ParameterGroupInfo(required=True, ui_position=2)],
            ),
            CommandParameter(
                name="action",
                cli_name="Action",
                display_name="Action",
                type=ParameterType.ChooseOne,
                choices=["start", "stop"],
                default_value="start",
                description="Start or stop the reverse port forward.",
                parameter_group_info=[ParameterGroupInfo(required=False, ui_position=3)],
            ),
        ]

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if not cl:
            raise Exception(
                "rpfwd requires arguments, for example: rpfwd -Port 445 -RemoteIP 127.0.0.1 -RemotePort 8443"
            )

        if cl.startswith("{"):
            self.load_args_from_dictionary(json.loads(cl))
        else:
            tokens = shlex.split(cl)
            if len(tokens) == 1:
                self.add_arg("port", int(tokens[0]))
                self.add_arg("action", "start")
            else:
                i = 0
                while i < len(tokens):
                    key = tokens[i].lower()
                    if key in ("-port", "/port") and i + 1 < len(tokens):
                        self.add_arg("port", int(tokens[i + 1]))
                        i += 2
                    elif key in ("-remoteip", "/remoteip") and i + 1 < len(tokens):
                        self.add_arg("remote_ip", tokens[i + 1])
                        i += 2
                    elif key in ("-remoteport", "/remoteport") and i + 1 < len(tokens):
                        self.add_arg("remote_port", int(tokens[i + 1]))
                        i += 2
                    elif key in ("-action", "/action") and i + 1 < len(tokens):
                        self.add_arg("action", tokens[i + 1].lower())
                        i += 2
                    else:
                        raise Exception(f"unknown rpfwd argument: {tokens[i]}")

        port = int(self.get_arg("port") or 0)
        if port < 1 or port > 65535:
            raise Exception("port must be between 1 and 65535.")
        action = self.get_arg("action") or "start"
        if action not in ("start", "stop"):
            raise Exception("action must be start or stop.")
        self.set_arg("action", action)

        if action == "start":
            remote_ip = self.get_arg("remote_ip")
            remote_port = int(self.get_arg("remote_port") or 0)
            if not remote_ip:
                raise Exception("remote_ip is required when starting rpfwd.")
            if remote_port < 1 or remote_port > 65535:
                raise Exception("remote_port must be between 1 and 65535.")


class RpfwdCommand(CommandBase):
    cmd = "rpfwd"
    needs_admin = False
    help_cmd = "rpfwd -Port 445 -RemoteIP 127.0.0.1 -RemotePort 8443"
    description = "Listen on the target host and reverse-forward connections through Mythic."
    version = 1
    author = "@zumpyx"
    argument_class = RpfwdArguments
    attackmapping = ["T1090"]
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
        response = PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )
        action = taskData.args.get_arg("action") or "start"
        port = taskData.args.get_arg("port")
        remote_ip = taskData.args.get_arg("remote_ip") or ""
        remote_port = taskData.args.get_arg("remote_port") or 0

        if action == "start":
            response.DisplayParams = (
                f"-Port {port} -RemoteIP {remote_ip} -RemotePort {remote_port}"
            )
            rpc_resp = await SendMythicRPCProxyStartCommand(
                MythicRPCProxyStartMessage(
                    TaskID=taskData.Task.ID,
                    PortType="rpfwd",
                    LocalPort=port,
                    RemoteIP=remote_ip,
                    RemotePort=remote_port,
                )
            )
            success_text = (
                f"Starting rpfwd listener on agent port {port}; Mythic will connect to {remote_ip}:{remote_port}\n"
            )
        else:
            response.DisplayParams = f"-Action stop -Port {port}"
            rpc_resp = await SendMythicRPCProxyStopCommand(
                MythicRPCProxyStopMessage(
                    TaskID=taskData.Task.ID,
                    PortType="rpfwd",
                    Port=port,
                )
            )
            success_text = f"Stopping rpfwd listener on agent port {port}\n"

        if not rpc_resp.Success:
            response.TaskStatus = MythicStatus.Error
            response.Stderr = rpc_resp.Error
            response.Completed = True
            await SendMythicRPCResponseCreate(
                MythicRPCResponseCreateMessage(
                    TaskID=taskData.Task.ID,
                    Response=rpc_resp.Error.encode(),
                )
            )
            return response

        await SendMythicRPCResponseCreate(
            MythicRPCResponseCreateMessage(
                TaskID=taskData.Task.ID,
                Response=success_text.encode(),
            )
        )
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        from ._base import error_aware_process_response

        return error_aware_process_response(task, response)
