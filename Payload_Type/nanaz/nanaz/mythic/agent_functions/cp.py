import json

from mythic_container.MythicCommandBase import *

from ._base import (
    FileBrowserArguments,
    error_aware_process_response,
    simple_command_attributes,
    split_cli_preserve_backslashes,
)


class CpArguments(FileBrowserArguments):
    cli_takes_path = False  # cp takes two positional args (src dst), handled below
    command_name = "cp"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(name="src", type=ParameterType.String, default_value=""),
            CommandParameter(name="dst", type=ParameterType.String, default_value=""),
        ]

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if not cl:
            return
        if cl.startswith("{"):
            try:
                data = json.loads(cl)
            except json.JSONDecodeError as e:
                raise Exception(f"cp: invalid JSON: {e}")
            if not data.get("src") or not data.get("dst"):
                raise Exception("cp JSON requires 'src' and 'dst'.")
            self.set_arg("src", data["src"])
            self.set_arg("dst", data["dst"])
            return
        # Preserve Windows backslashes while still allowing quoted paths:
        # cp "/path with space/a.txt" "/path with space/b.txt" from the CLI.
        try:
            parts = split_cli_preserve_backslashes(cl)
        except ValueError as e:
            raise Exception(f"cp: failed to parse command line: {e}")
        if len(parts) < 2:
            raise Exception("cp requires source AND destination paths.")
        self.set_arg("src", parts[0])
        self.set_arg("dst", parts[1])


class CpCommand(CommandBase):
    cmd = "cp"
    needs_admin = False
    help_cmd = "cp [src] [dst]"
    description = "Copy a file from source to destination. Cross-platform."
    version = 1
    author = "@zumpyx"
    argument_class = CpArguments
    attackmapping = ["T1105"]
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        response.DisplayParams = f"{taskData.args.get_arg('src')} → {taskData.args.get_arg('dst')}"
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
