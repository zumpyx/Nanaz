import base64

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *

from ._base import FileBrowserArguments, simple_command_attributes


class UploadArguments(FileBrowserArguments):
    command_name = "upload"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        # `host` is intentionally not a CommandParameter — Mythic's UI
        # injects it via the {host, full_path} dict, and the CLI never
        # has to type it. (The Rust Params struct previously accepted
        # it for symmetry with download; that path is now #[allow
        # (dead_code)] and slated for removal once all callers migrate
        # to the host-free form.)
        self.args = [
            CommandParameter(name="path", type=ParameterType.String, default_value=""),
            CommandParameter(name="file", type=ParameterType.File),
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
            file_search = await SendMythicRPCFileSearch(
                MythicRPCFileSearchMessage(
                    TaskID=taskData.Task.ID,
                    AgentFileID=file_uuid,
                )
            )
            original_filename = ""
            if file_search.Success and len(file_search.Files) > 0:
                original_filename = file_search.Files[0].Filename
            file_resp = await SendMythicRPCFileGetContent(
                MythicRPCFileGetContentMessage(file_uuid)
            )
            if file_resp.Success and file_resp.Content is not None:
                encoded = base64.b64encode(file_resp.Content).decode("utf-8")
                taskData.args.add_arg("file_bytes", encoded)
                if original_filename:
                    taskData.args.add_arg("original_filename", original_filename)
                taskData.args.remove_arg("file")
                shown = dest_path or original_filename
                response.DisplayParams = f"{shown} ({len(file_resp.Content)} bytes)"
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
