import base64

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *

from ._base import FileBrowserArguments, simple_command_attributes


class UploadArguments(FileBrowserArguments):
    command_name = "upload"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(name="path", type=ParameterType.String, default_value=""),
            CommandParameter(name="file", type=ParameterType.File),
            CommandParameter(name="host", type=ParameterType.String, default_value=""),
        ]


class UploadCommand(CommandBase):
    cmd = "upload"
    needs_admin = False
    help_cmd = "upload [destination_path]"
    description = "Upload a file to the target. File content is embedded as base64."
    version = 1
    author = "@zumpyx"
    argument_class = UploadArguments
    attackmapping = ["T1105", "T1036"]
    supported_ui_features = ["file_browser:upload"]
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)

        file_uuid = taskData.args.get_arg("file")
        dest_path = taskData.args.get_arg("path")

        try:
            file_resp = await SendMythicRPCFileGetContent(
                MythicRPCFileGetContentMessage(file_uuid)
            )
            if file_resp.Success and file_resp.Content is not None:
                encoded = base64.b64encode(file_resp.Content).decode("utf-8")
                taskData.args.add_arg("file_bytes", encoded)
                taskData.args.remove_arg("file")
                response.DisplayParams = f"{dest_path} ({len(file_resp.Content)} bytes)"
            else:
                response.Success = False
                response.Error = file_resp.Error or "Failed to fetch file content"
                return response
        except Exception as e:
            response.Success = False
            response.Error = f"Error fetching upload file: {str(e)}"
            return response

        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        from ._base import error_aware_process_response
        return error_aware_process_response(task, response)
