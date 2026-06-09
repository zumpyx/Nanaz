from mythic_container.MythicCommandBase import *


class EnvArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(name="key", type=ParameterType.String, default_value=""),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        key = self.command_line.strip()
        if key:
            self.set_arg("key", key)


class EnvCommand(CommandBase):
    cmd = "env"
    needs_admin = False
    help_cmd = "env [filter_key]"
    description = "List environment variables, optionally filtered by key. Cross-platform."
    version = 1
    author = "@zumpyx"
    argument_class = EnvArguments
    attackmapping = ["T1082"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=False,
    )

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        key = taskData.args.get_arg("key")
        response.DisplayParams = key if key else "(all)"
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        from ._base import error_aware_process_response
        return error_aware_process_response(task, response)
