from mythic_container.MythicCommandBase import *

from ._base import FileBrowserArguments, simple_command_attributes


class WgetArguments(FileBrowserArguments):
    cli_takes_path = False  # wget takes (url, [path]) — handled below
    command_name = "wget"

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

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if not cl:
            return
        parts = cl.split(maxsplit=1)
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
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        url = taskData.args.get_arg("url")
        path = taskData.args.get_arg("path")
        response.DisplayParams = f"{url}" + (f" → {path}" if path else "")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
