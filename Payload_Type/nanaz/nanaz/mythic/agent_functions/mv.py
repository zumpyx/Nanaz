from mythic_container.MythicCommandBase import *


class MvArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="src",
                type=ParameterType.String,
                default_value="",
                parameter_group_info=[ParameterGroupInfo(ui_position=0, required=True)],
            ),
            CommandParameter(
                name="dst",
                type=ParameterType.String,
                default_value="",
                parameter_group_info=[ParameterGroupInfo(ui_position=1, required=True)],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise Exception("mv requires source and destination.")
        parts = self.command_line.strip().split(maxsplit=1)
        if len(parts) < 2:
            raise Exception("mv requires source AND destination.")
        self.set_arg("src", parts[0])
        self.set_arg("dst", parts[1])


class MvCommand(CommandBase):
    cmd = "mv"
    needs_admin = False
    help_cmd = "mv [src] [dst]"
    description = "Move/rename a file from source to destination. Cross-platform."
    version = 1
    author = "@zumpyx"
    argument_class = MvArguments
    attackmapping = ["T1105"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=False,
    )

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        response.DisplayParams = f"{taskData.args.get_arg('src')} → {taskData.args.get_arg('dst')}"
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
