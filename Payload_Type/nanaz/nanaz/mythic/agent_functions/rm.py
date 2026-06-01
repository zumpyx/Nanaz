import json

from mythic_container.MythicCommandBase import *

from ._base import FileBrowserArguments, simple_command_attributes


class RmArguments(FileBrowserArguments):
    cli_takes_path = True
    command_name = "rm"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(name="path", type=ParameterType.String, default_value=""),
            CommandParameter(name="recursive", type=ParameterType.Boolean, default_value=False),
        ]

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if not cl or cl.startswith("{"):
            await super().parse_arguments()
            return
        parts = cl.split(maxsplit=1)
        self.set_arg("path", parts[0])
        if len(parts) > 1 and parts[1].lower() in ("-r", "-rf", "/s"):
            self.set_arg("recursive", True)


class RmCommand(CommandBase):
    cmd = "rm"
    needs_admin = False
    help_cmd = "rm [path] [-r]"
    description = "Remove a file or directory (-r). Cross-platform."
    version = 1
    author = "@zumpyx"
    argument_class = RmArguments
    attackmapping = ["T1070"]
    supported_ui_features = ["file_browser:remove"]
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        path = taskData.args.get_arg("path")
        rec = taskData.args.get_arg("recursive")
        response.DisplayParams = f"{path}" + (" -r" if rec else "")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
