import base64
import os

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *


class UploadArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="path",
                type=ParameterType.String,
                default_value="",
                parameter_group_info=[
                    ParameterGroupInfo(
                        ui_position=0,
                        required=True,
                    )
                ],
            ),
            CommandParameter(
                name="file",
                type=ParameterType.File,
                parameter_group_info=[
                    ParameterGroupInfo(
                        ui_position=1,
                        required=True,
                    )
                ],
            ),
            CommandParameter(
                name="host",
                type=ParameterType.String,
                default_value="",
                parameter_group_info=[
                    ParameterGroupInfo(
                        ui_position=2,
                        required=False,
                    )
                ],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise Exception("upload requires a destination path.")
        self.set_arg("path", self.command_line.strip())


class UploadCommand(CommandBase):
    cmd = "upload"
    needs_admin = False
    help_cmd = "upload [destination_path]"
    description = "Upload a file to the target. File is base64-encoded in task parameters."
    version = 1
    author = "@zumpyx"
    argument_class = UploadArguments
    attackmapping = ["T1105", "T1036"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=False,
        supported_ui_features=["file_browser:upload"],
    )

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )

        file_uuid = taskData.args.get_arg("file")
        dest_path = taskData.args.get_arg("path")

        # Fetch file contents from Mythic and embed as base64
        try:
            file_resp = await SendMythicRPCFileGetContent(
                AgentFileUUID=file_uuid
            )
            if file_resp.Success and file_resp.Content is not None:
                file_bytes = file_resp.Content
                encoded = base64.b64encode(file_bytes).decode("utf-8")
                taskData.args.add_arg("file_bytes", encoded)
                taskData.args.remove_arg("file")
                response.DisplayParams = f"{dest_path} ({len(file_bytes)} bytes)"
            else:
                response.Success = False
                response.Error = file_resp.Error or "Failed to fetch file content from Mythic"
                return response
        except Exception as e:
            response.Success = False
            response.Error = f"Error fetching upload file: {str(e)}"
            return response

        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        return resp
