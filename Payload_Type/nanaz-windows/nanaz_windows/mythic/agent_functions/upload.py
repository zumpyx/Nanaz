import json

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *

from ._base import (
    FileBrowserArguments,
    _strip_outer_quotes,
    normalize_browser_path,
    simple_command_attributes,
)


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
            CommandParameter(
                name="max_bytes",
                type=ParameterType.Number,
                default_value=268435456,
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        clean_args = {}
        if dictionary_arguments.get("full_path"):
            clean_args["path"] = normalize_browser_path(
                str(dictionary_arguments["full_path"])
            )
        elif "path" in dictionary_arguments:
            clean_args["path"] = normalize_browser_path(str(dictionary_arguments["path"]))
        if "file" in dictionary_arguments:
            clean_args["file"] = dictionary_arguments["file"]
        if "max_bytes" in dictionary_arguments:
            clean_args["max_bytes"] = dictionary_arguments["max_bytes"]
        self.load_args_from_dictionary(clean_args)
        if dictionary_arguments.get("host"):
            self.add_arg("host", dictionary_arguments["host"])

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if not cl:
            return
        if cl.startswith("{"):
            try:
                await self.parse_dictionary(json.loads(cl))
                return
            except json.JSONDecodeError:
                pass
        self.set_arg("path", _strip_outer_quotes(cl))


class UploadCommand(CommandBase):
    cmd = "upload"
    needs_admin = False
    help_cmd = "upload [destination_path]"
    description = "Upload a file to the target using Mythic file chunk transfer."
    version = 1
    author = "@zumpyx"
    argument_class = UploadArguments
    attackmapping = ["T1105", "T1036"]
    supported_ui_features = ["file_browser:upload"]
    attributes = simple_command_attributes(suggested_command=True)

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)

        file_uuid = taskData.args.get_arg("file")
        dest_path = taskData.args.get_arg("path")
        if not taskData.args.get_arg("host"):
            taskData.args.add_arg("host", taskData.Callback.Host)

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
            taskData.args.add_arg("file_id", file_uuid)
            if original_filename:
                taskData.args.add_arg("original_filename", original_filename)
            taskData.args.remove_arg("file")
            shown = dest_path or original_filename or file_uuid
            response.DisplayParams = f"{shown} ({file_uuid})"
        except Exception as e:
            response.Success = False
            response.Error = f"Error fetching upload file: {str(e)}"
            return response

        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        from ._base import error_aware_process_response
        return error_aware_process_response(task, response)
