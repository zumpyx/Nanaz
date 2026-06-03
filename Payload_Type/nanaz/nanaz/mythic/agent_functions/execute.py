from mythic_container.MythicCommandBase import *

from ._base import (
    error_aware_process_response,
    read_cli_token,
)


class ExecuteArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(name="path", type=ParameterType.String, default_value=""),
            CommandParameter(name="arguments", type=ParameterType.String, default_value=""),
            CommandParameter(name="timeout", type=ParameterType.Number, default_value=60),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if not cl:
            raise Exception("execute requires a path.")
        if cl.startswith("{"):
            self.load_args_from_json_string(cl)
            return
        path, _, end = read_cli_token(cl)
        if not path:
            raise Exception("execute requires a path.")
        self.set_arg("path", path)
        self.set_arg("arguments", cl[end:].strip())


class ExecuteCommand(CommandBase):
    cmd = "execute"
    needs_admin = False
    help_cmd = "execute [path] [arguments]"
    description = "Execute a program directly without a shell."
    version = 1
    author = "@zumpyx"
    argument_class = ExecuteArguments
    attackmapping = ["T1106"]
    supported_ui_features = ["execute", "execute:process"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=True,
    )

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        path = taskData.args.get_arg("path")
        arguments = taskData.args.get_arg("arguments")
        timeout = taskData.args.get_arg("timeout")
        display = path if not arguments else f"{path} {arguments}"
        return PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
            DisplayParams=f"-timeout {timeout} {display}",
        )

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
