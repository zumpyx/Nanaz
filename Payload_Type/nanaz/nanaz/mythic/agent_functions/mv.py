import json

from mythic_container.MythicCommandBase import *

from ._base import (
    FileBrowserArguments,
    error_aware_process_response,
    simple_command_attributes,
    split_cli_preserve_backslashes,
)


class MvArguments(FileBrowserArguments):
    cli_takes_path = False
    command_name = "mv"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(name="src", type=ParameterType.String, default_value=""),
            CommandParameter(name="dst", type=ParameterType.String, default_value=""),
            CommandParameter(
                name="allow_system_path",
                type=ParameterType.Boolean,
                default_value=False,
            ),
            CommandParameter(
                name="allow_source_system_path",
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
                raise Exception(f"mv: invalid JSON: {e}")
            if not data.get("src") or not data.get("dst"):
                raise Exception("mv JSON requires 'src' and 'dst'.")
            self.set_arg("src", data["src"])
            self.set_arg("dst", data["dst"])
            if "allow_system_path" in data:
                self.set_arg("allow_system_path", data["allow_system_path"])
            if "allow_source_system_path" in data:
                self.set_arg("allow_source_system_path", data["allow_source_system_path"])
            return
        # Preserve Windows backslashes while still allowing quoted paths.
        try:
            parts = split_cli_preserve_backslashes(cl)
        except ValueError as e:
            raise Exception(f"mv: failed to parse command line: {e}")
        if len(parts) < 2:
            raise Exception("mv requires source AND destination.")
        self.set_arg("src", parts[0])
        self.set_arg("dst", parts[1])


class MvCommand(CommandBase):
    cmd = "mv"
    needs_admin = False
    help_cmd = "mv [src] [dst]"
    description = "Move/rename a file from source to destination. Cross-platform."
    version = 1
    author = "@zumpyx"
    argument_class = MvArguments
    attackmapping = ["T1105"]
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        response.DisplayParams = f"{taskData.args.get_arg('src')} → {taskData.args.get_arg('dst')}"
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
