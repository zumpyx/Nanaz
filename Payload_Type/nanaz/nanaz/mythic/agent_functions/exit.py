import json

from mythic_container.MythicCommandBase import *


class ExitArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="method",
                type=ParameterType.ChooseOne,
                choices=["process", "thread"],
                default_value="process",
                parameter_group_info=[
                    ParameterGroupInfo(
                        ui_position=0,
                        required=True,
                    )
                ],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        method = self.command_line.strip().lower()
        if method in ("process", "thread"):
            self.set_arg("method", method)
        elif method:
            raise Exception(f"exit requires 'process' or 'thread', got: {method}")


class ExitCommand(CommandBase):
    cmd = "exit"
    needs_admin = False
    help_cmd = "exit [process|thread]"
    description = "Exit the implant — process (kill entire process) or thread (stop beacon loop only)."
    version = 1
    author = "@zumpyx"
    argument_class = ExitArguments
    attackmapping = ["T1029"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=False,
    )

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )
        method = taskData.args.get_arg("method")
        response.DisplayParams = f"-method {method}"
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        return resp
