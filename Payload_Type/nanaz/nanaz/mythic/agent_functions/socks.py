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


class SocksArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="port",
                cli_name="Port",
                display_name="Port",
                type=ParameterType.Number,
                description="Port to bind on the Mythic server. Use 0 to let Mythic choose.",
                parameter_group_info=[ParameterGroupInfo(required=True, ui_position=0)],
            ),
            CommandParameter(
                name="action",
                cli_name="Action",
                display_name="Action",
                type=ParameterType.ChooseOne,
                choices=["start", "stop"],
                default_value="start",
                description="Start or stop the SOCKS5 listener.",
                parameter_group_info=[ParameterGroupInfo(required=False, ui_position=1)],
            ),
            CommandParameter(
                name="username",
                cli_name="Username",
                display_name="Username",
                type=ParameterType.String,
                description="Optional username required by Mythic's SOCKS listener.",
                parameter_group_info=[ParameterGroupInfo(required=False, ui_position=2)],
            ),
            CommandParameter(
                name="password",
                cli_name="Password",
                display_name="Password",
                type=ParameterType.String,
                description="Optional password required by Mythic's SOCKS listener.",
                parameter_group_info=[ParameterGroupInfo(required=False, ui_position=3)],
            ),
        ]

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if not cl:
            raise Exception("socks requires a port, for example: socks 7000")

        if cl.startswith("{"):
            data = json.loads(cl)
            self.load_args_from_dictionary(data)
        elif cl.startswith("-") or cl.startswith("/"):
            tokens = shlex.split(cl)
            i = 0
            while i < len(tokens):
                key = tokens[i].lower()
                if key in ("-port", "/port") and i + 1 < len(tokens):
                    self.add_arg("port", int(tokens[i + 1]))
                    i += 2
                elif key in ("-action", "/action") and i + 1 < len(tokens):
                    self.add_arg("action", tokens[i + 1].lower())
                    i += 2
                elif key in ("-username", "/username") and i + 1 < len(tokens):
                    self.add_arg("username", tokens[i + 1])
                    i += 2
                elif key in ("-password", "/password") and i + 1 < len(tokens):
                    self.add_arg("password", tokens[i + 1])
                    i += 2
                else:
                    raise Exception(f"unknown socks argument: {tokens[i]}")
        else:
            self.add_arg("port", int(cl))
            self.add_arg("action", "start")

        port = int(self.get_arg("port"))
        if port < 0 or port > 65535:
            raise Exception("port must be between 0 and 65535.")

        action = self.get_arg("action") or "start"
        if action not in ("start", "stop"):
            raise Exception("action must be start or stop.")
        self.set_arg("action", action)


class SocksCommand(CommandBase):
    cmd = "socks"
    needs_admin = False
    help_cmd = "socks -Port 7000 -Action start"
    description = "Start or stop a Mythic SOCKS5 listener routed through nanaz."
    version = 1
    script_only = True
    author = "@zumpyx"
    argument_class = SocksArguments
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
        port = taskData.args.get_arg("port")
        action = taskData.args.get_arg("action") or "start"
        username = taskData.args.get_arg("username") or ""
        password = taskData.args.get_arg("password") or ""

        response.DisplayParams = f"-Action {action} -Port {port}"
        if username:
            response.DisplayParams += f" -Username {username}"

        if action == "start":
            rpc_resp = await SendMythicRPCProxyStartCommand(
                MythicRPCProxyStartMessage(
                    TaskID=taskData.Task.ID,
                    PortType="socks",
                    LocalPort=port,
                    Username=username,
                    Password=password,
                )
            )
            success_text = f"Started SOCKS5 server on port {port}\n"
        else:
            rpc_resp = await SendMythicRPCProxyStopCommand(
                MythicRPCProxyStopMessage(
                    TaskID=taskData.Task.ID,
                    PortType="socks",
                    Port=port,
                    Username=username,
                    Password=password,
                )
            )
            success_text = f"Stopped SOCKS5 server on port {port}\n"

        if not rpc_resp.Success:
            response.TaskStatus = MythicStatus.Error
            response.Stderr = rpc_resp.Error
            await SendMythicRPCResponseCreate(
                MythicRPCResponseCreateMessage(
                    TaskID=taskData.Task.ID,
                    Response=rpc_resp.Error.encode(),
                )
            )
            return response

        response.TaskStatus = MythicStatus.Success
        response.Completed = True
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
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
