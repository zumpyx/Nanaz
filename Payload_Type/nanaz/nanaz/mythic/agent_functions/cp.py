from mythic_container.MythicCommandBase import *

from ._base import FileBrowserArguments, simple_command_attributes


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
        parts = cl.split(maxsplit=1)
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
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
