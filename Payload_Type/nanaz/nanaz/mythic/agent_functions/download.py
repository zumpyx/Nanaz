from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *

from ._base import FileBrowserArguments, simple_command_attributes


class DownloadArguments(FileBrowserArguments):
    command_name = "download"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(name="path", type=ParameterType.String, default_value=""),
            CommandParameter(name="host", type=ParameterType.String, default_value=""),
        ]


class DownloadCommand(CommandBase):
    cmd = "download"
    needs_admin = False
    help_cmd = "download [path]"
    description = "Download a file from target. Multi-chunk streaming (no 50MB cap)."
    version = 1
    author = "@zumpyx"
    argument_class = DownloadArguments
    attackmapping = ["T1041", "T1105"]
    supported_ui_features = ["file_browser:download"]
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        response.DisplayParams = taskData.args.get_arg("path")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
