from mythic_container.MythicCommandBase import *

from ._base import error_aware_process_response, validate_timeout


class PowerShellArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(name="command", type=ParameterType.String, default_value=""),
            CommandParameter(name="timeout", type=ParameterType.Number, default_value=60),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        if not self.command_line.strip():
            raise Exception("powershell requires a command.")
        self.set_arg("command", self.command_line)


class PowerShellCommand(CommandBase):
    cmd = "powershell"
    needs_admin = False
    help_cmd = "powershell [command]"
    description = "Execute a command through powershell.exe -Command."
    version = 1
    author = "@zumpyx"
    argument_class = PowerShellArguments
    attackmapping = ["T1059.001"]
    supported_ui_features = ["shell", "shell:powershell", "execute:shell"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows],
        builtin=False,
        load_only=False,
        suggested_command=True,
    )

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        command = taskData.args.get_arg("command")
        timeout = taskData.args.get_arg("timeout")
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
            DisplayParams=f"-timeout {timeout} {command}",
        )

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
