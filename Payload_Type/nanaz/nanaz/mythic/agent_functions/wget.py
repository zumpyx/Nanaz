from mythic_container.MythicCommandBase import *


class WgetArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="url",
                type=ParameterType.String,
                default_value="",
            ),
            CommandParameter(
                name="path",
                type=ParameterType.String,
                default_value="",
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise Exception("wget requires a URL.")
        parts = self.command_line.strip().split(maxsplit=1)
        self.set_arg("url", parts[0])
        if len(parts) > 1:
            self.set_arg("path", parts[1])


class WgetCommand(CommandBase):
    cmd = "wget"
    needs_admin = False
    help_cmd = "wget [url] [destination_path]"
    description = "Download a file from a URL. Cross-platform."
    version = 1
    author = "@zumpyx"
    argument_class = WgetArguments
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
        url = taskData.args.get_arg("url")
        path = taskData.args.get_arg("path")
        response.DisplayParams = f"{url}" + (f" → {path}" if path else "")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
