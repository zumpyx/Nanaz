import json

from mythic_container.MythicCommandBase import *


class RmArguments(TaskArguments):
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
                name="recursive",
                type=ParameterType.Boolean,
                default_value=False,
                parameter_group_info=[ParameterGroupInfo(ui_position=1, required=False)],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        """File browser sends {host, path, file, full_path}. Use full_path."""
        if "host" in dictionary_arguments and dictionary_arguments.get("full_path"):
            self.set_arg("path", dictionary_arguments["full_path"])
        else:
            self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        cl = self.command_line.strip()
        if len(cl) == 0:
            raise Exception("rm requires a path.")
        # Handle file browser UI JSON
        if cl.startswith("{"):
            try:
                data = json.loads(cl)
                if "host" in data and data.get("full_path"):
                    self.set_arg("path", data["full_path"])
                    return
            except Exception:
                pass
        parts = cl.split(maxsplit=1)
        self.set_arg("path", parts[0])
        if len(parts) > 1 and parts[1].lower() in ("-r", "-rf", "/s"):
            self.set_arg("recursive", True)


class RmCommand(CommandBase):
    cmd = "rm"
    needs_admin = False
    help_cmd = "rm [path] [-r]"
    description = "Remove a file or directory (-r). Cross-platform."
    version = 1
    author = "@zumpyx"
    argument_class = RmArguments
    attackmapping = ["T1070"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=False,
        supported_ui_features=["file_browser:remove"],
    )

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        path = taskData.args.get_arg("path")
        rec = taskData.args.get_arg("recursive")
        response.DisplayParams = f"{path}" + (" -r" if rec else "")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
