import json

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *


class LsArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="path",
                type=ParameterType.String,
                default_value=".",
                parameter_group_info=[
                    ParameterGroupInfo(
                        ui_position=0,
                        required=True,
                    )
                ],
            ),
            CommandParameter(
                name="host",
                type=ParameterType.String,
                default_value="",
                parameter_group_info=[
                    ParameterGroupInfo(
                        ui_position=1,
                        required=False,
                    )
                ],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            self.set_arg("path", ".")
            return
        # Handle file browser UI tasking: {"host":..., "path":..., "file":..., "full_path":...}
        cl = self.command_line.strip()
        if cl.startswith("{"):
            try:
                data = json.loads(cl)
                if "host" in data:
                    # File browser sends path=parent, file=name, full_path=absolute
                    if data.get("full_path"):
                        self.set_arg("path", data["full_path"])
                    elif data.get("path"):
                        fname = data.get("file", "")
                        self.set_arg("path", data["path"].rstrip("/") + "/" + fname)
                    if data.get("host"):
                        self.set_arg("host", data["host"])
                    return
                elif "path" in data:
                    self.set_arg("path", data["path"])
                    return
            except Exception:
                pass
        self.set_arg("path", cl)


class LsCommand(CommandBase):
    cmd = "ls"
    needs_admin = False
    help_cmd = "ls [path]"
    description = "List files and directories at the given path. Integrates with Mythic's file browser UI."
    version = 1
    author = "@zumpyx"
    argument_class = LsArguments
    attackmapping = ["T1083", "T1105"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=False,
        supported_ui_features=["file_browser:list"],
    )

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )
        path = taskData.args.get_arg("path")
        response.DisplayParams = path
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        return resp
