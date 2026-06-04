import json

from mythic_container.MythicCommandBase import *

from ._base import (
    FileBrowserArguments,
    simple_command_attributes,
    split_cli_preserve_backslashes,
)


class WgetArguments(FileBrowserArguments):
    cli_takes_path = False  # wget takes (url, [path]) — handled below
    command_name = "wget"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="url",
                type=ParameterType.String,
                default_value="",
            ),
            CommandParameter(
                name="path",
                type=ParameterType.String,
                default_value="",
            ),
            CommandParameter(
                name="max_bytes",
                type=ParameterType.Number,
                default_value=268435456,
            ),
            CommandParameter(
                name="allow_system_path",
                type=ParameterType.Boolean,
                default_value=False,
            ),
        ]

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if not cl:
            return
        if cl.startswith("{"):
            try:
                data = json.loads(cl)
            except json.JSONDecodeError as e:
                raise Exception(f"wget: invalid JSON: {e}")
            if not data.get("url"):
                raise Exception("wget JSON requires 'url'.")
            self.set_arg("url", data["url"])
            if data.get("path"):
                self.set_arg("path", data["path"])
            for key in ("max_bytes", "allow_system_path"):
                if key in data:
                    self.set_arg(key, data[key])
            return
        # Preserve Windows backslashes while still allowing quotes around the
        # destination path (common — operators often type "/tmp/My Drop/x").
        try:
            parts = split_cli_preserve_backslashes(cl)
        except ValueError as e:
            raise Exception(f"wget: failed to parse command line: {e}")
        allow_system_path = False
        filtered = []
        for part in parts:
            if part == "--allow-system-path":
                allow_system_path = True
            else:
                filtered.append(part)
        if not filtered:
            raise Exception("wget requires a URL.")
        self.set_arg("url", filtered[0])
        if len(filtered) > 1:
            self.set_arg("path", filtered[1])
        if allow_system_path:
            self.set_arg("allow_system_path", True)


class WgetCommand(CommandBase):
    cmd = "wget"
    needs_admin = False
    help_cmd = "wget [url] [destination_path]"
    description = "Download a file from a URL. Cross-platform."
    version = 1
    author = "@zumpyx"
    argument_class = WgetArguments
    attackmapping = ["T1105"]
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        url = taskData.args.get_arg("url")
        path = taskData.args.get_arg("path")
        response.DisplayParams = f"{url}" + (f" -> {path}" if path else "")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        from ._base import error_aware_process_response
        return error_aware_process_response(task, response)
