from mythic_container.MythicCommandBase import *

from ._base import (
    error_aware_process_response,
    simple_command_attributes,
    split_cli_preserve_backslashes,
)
from .ls import LsArguments


class TreeArguments(LsArguments):
    command_name = "tree"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args.append(
            CommandParameter(
                name="depth",
                type=ParameterType.Number,
                default_value=3,
                description="Maximum recursion depth from the starting path.",
                parameter_group_info=[
                    ParameterGroupInfo(
                        required=False,
                        group_name="Default",
                        ui_position=2,
                    )
                ],
            )
        )

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if not cl:
            return
        if cl.startswith("{"):
            self.load_args_from_json_string(cl)
            return
        parts = split_cli_preserve_backslashes(cl)
        if parts:
            self.set_arg("path", parts[0])
        if len(parts) > 1:
            try:
                depth = int(parts[1])
            except ValueError:
                raise Exception("tree depth must be an integer.")
            if depth < 0:
                raise Exception("tree depth must be >= 0.")
            self.set_arg("depth", depth)


class TreeCommand(CommandBase):
    cmd = "tree"
    needs_admin = False
    help_cmd = "tree [path]"
    description = "Recursively list files and directories from a path."
    version = 1
    author = "@zumpyx"
    argument_class = TreeArguments
    attackmapping = ["T1083"]
    supported_ui_features = ["file_browser:tree"]
    browser_script = None
    attributes = simple_command_attributes(suggested_command=True)

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        path = taskData.args.get_arg("path") or "."
        depth = taskData.args.get_arg("depth")
        if depth is None:
            depth = 3
        if depth < 0:
            response.Success = False
            response.Error = "tree depth must be >= 0."
            return response
        response.DisplayParams = f"{path} -depth {depth}"
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
