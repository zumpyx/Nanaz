from mythic_container.MythicCommandBase import *


class ShellArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="command",
                type=ParameterType.String,
                default_value="",
                parameter_group_info=[
                    ParameterGroupInfo(
                        ui_position=0,
                        required=True,
                    )
                ],
            ),
            CommandParameter(
                name="shell",
                type=ParameterType.ChooseOne,
                choices=["cmd", "powershell", "bash", "sh"],
                default_value="sh",
                parameter_group_info=[
                    ParameterGroupInfo(
                        ui_position=1,
                        required=True,
                    )
                ],
            ),
            CommandParameter(
                name="timeout",
                type=ParameterType.Number,
                default_value=60,
                parameter_group_info=[
                    ParameterGroupInfo(
                        ui_position=2,
                        required=False,
                    )
                ],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise Exception("shell requires a command to execute.")
        self.set_arg("command", self.command_line)


class ShellCommand(CommandBase):
    cmd = "shell"
    needs_admin = False
    help_cmd = "shell [command]"
    description = "Execute a shell command with timeout (default 60s)."
    version = 1
    author = "@zumpyx"
    argument_class = ShellArguments
    attackmapping = ["T1059"]
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
        command = taskData.args.get_arg("command")
        shell = taskData.args.get_arg("shell")
        timeout = taskData.args.get_arg("timeout")
        response.DisplayParams = f"-shell {shell} -timeout {timeout} {command}"
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        from ._base import error_aware_process_response
        return error_aware_process_response(task, response)
