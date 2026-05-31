import base64
import json

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
                parameter_group_info=[ParameterGroupInfo(ui_position=0, required=True)],
            ),
            CommandParameter(
                name="file",
                type=ParameterType.File,
                parameter_group_info=[ParameterGroupInfo(ui_position=1, required=True)],
            ),
            CommandParameter(
                name="host",
                type=ParameterType.String,
                default_value="",
                parameter_group_info=[ParameterGroupInfo(ui_position=2, required=False)],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        """File browser may send {host, path, file, full_path}. Use full_path as dest."""
        if "host" in dictionary_arguments and dictionary_arguments.get("full_path"):
            self.set_arg("path", dictionary_arguments["full_path"])
        else:
            self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if cl.startswith("{"):
            try:
                data = json.loads(cl)
                if "host" in data and data.get("full_path"):
                    self.set_arg("path", data["full_path"])
                    return
                elif "path" in data:
                    self.set_arg("path", data["path"])
                    return
            except Exception:
                pass
        if len(cl) == 0:
            raise Exception("upload requires a destination path.")
        self.set_arg("path", cl)


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
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=False,
    )

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
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
