from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *

from ._base import FileBrowserArguments, simple_command_attributes


class DownloadArguments(FileBrowserArguments):
    command_name = "download"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        # `host` is intentionally not a CommandParameter — Mythic's UI
        # injects it via the {host, full_path} dict that flows through
        # FileBrowserArguments.parse_dictionary, and the CLI never has
        # to type it. The Rust handler accepts the hidden field only so
        # Mythic can tag downloads with the originating callback host.
        self.args = [
            CommandParameter(name="path", type=ParameterType.String, default_value=""),
            CommandParameter(
                name="chunk_size",
                type=ParameterType.Number,
                default_value=524288,
            ),
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
    attributes = simple_command_attributes(suggested_command=True)

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        if not taskData.args.get_arg("host"):
            taskData.args.add_arg("host", taskData.Callback.Host)
        response.DisplayParams = taskData.args.get_arg("path")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        from ._base import error_aware_process_response
        return error_aware_process_response(task, response)
