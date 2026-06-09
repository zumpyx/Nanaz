from mythic_container.MythicCommandBase import *

from ._base import error_aware_process_response, simple_command_attributes


class DrivesArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = []

    async def parse_arguments(self):
        if self.command_line.strip():
            raise Exception("drives takes no arguments.")


class DrivesCommand(CommandBase):
    cmd = "drives"
    needs_admin = False
    help_cmd = "drives"
    description = "List available filesystem roots / drives."
    version = 1
    author = "@zumpyx"
    argument_class = DrivesArguments
    attackmapping = ["T1083"]
    supported_ui_features = ["file_browser:list_roots"]
    attributes = simple_command_attributes(suggested_command=True)

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        return PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
            DisplayParams="",
        )

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
