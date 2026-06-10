from mythic_container.MythicCommandBase import *

from ._base import error_aware_process_response, validate_timeout


class PowerPickArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="command",
                cli_name="Command",
                display_name="Command",
                type=ParameterType.String,
                description="PowerShell command to execute in an isolated rustclr worker.",
                parameter_group_info=[
                    ParameterGroupInfo(
                        required=True,
                        group_name="Default",
                        ui_position=1,
                    )
                ],
            ),
            CommandParameter(
                name="timeout",
                cli_name="Timeout",
                display_name="Timeout",
                type=ParameterType.Number,
                default_value=300,
                description="Maximum worker runtime in seconds.",
                parameter_group_info=[
                    ParameterGroupInfo(
                        required=False,
                        group_name="Default",
                        ui_position=2,
                    )
                ],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        if not self.command_line.strip():
            raise Exception(f"powerpick requires a command.\n\tUsage: {PowerPickCommand.help_cmd}")
        self.set_arg("command", self.command_line)


class PowerPickCommand(CommandBase):
    cmd = "powerpick"
    needs_admin = False
    help_cmd = "powerpick [command]"
    description = "Execute PowerShell in an isolated rustclr worker. Explicitly select this command only when CLR execution risk is acceptable."
    version = 1
    author = "@zumpyx"
    argument_class = PowerPickArguments
    attackmapping = ["T1059.001"]
    supported_ui_features = ["powerpick", "execute:powershell", "execute:dotnet"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows],
        builtin=False,
        load_only=False,
        suggested_command=False,
    )

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        command = taskData.args.get_arg("command")
        timeout = taskData.args.get_arg("timeout") or 300
        timeout_error = validate_timeout(timeout)
        if timeout_error:
            return PTTaskCreateTaskingMessageResponse(
                TaskID=taskData.Task.ID,
                Success=False,
                Error=timeout_error,
            )
        return PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
            DisplayParams=f"-Timeout {timeout} {command}",
        )

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
