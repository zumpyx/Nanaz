from mythic_container.MythicCommandBase import *

from ._base import FileBrowserArguments, error_aware_process_response, simple_command_attributes


class CatArguments(FileBrowserArguments):
    command_name = "cat"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="path",
                type=ParameterType.String,
                default_value="",
            ),
        ]


class CatCommand(CommandBase):
    cmd = "cat"
    needs_admin = False
    help_cmd = "cat [path]"
    description = "Read and display file contents. Cross-platform."
    version = 1
    author = "@zumpyx"
    argument_class = CatArguments
    attackmapping = ["T1005"]
    supported_ui_features = ["cat"]
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        response.DisplayParams = taskData.args.get_arg("path")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
