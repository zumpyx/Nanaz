from mythic_container.MythicCommandBase import *

from ._base import error_aware_process_response, simple_command_attributes


class HelpArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="command",
                type=ParameterType.String,
                default_value="",
                description="Optional command name to show detailed help for.",
                parameter_group_info=[
                    ParameterGroupInfo(required=False, ui_position=0),
                ],
            ),
        ]

    async def parse_arguments(self):
        command = self.command_line.strip()
        if command:
            self.set_arg("command", command)

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class HelpCommand(CommandBase):
    cmd = "help"
    needs_admin = False
    help_cmd = "help [command]"
    description = "Show nanaz command help from the agent."
    version = 1
    author = "@zumpyx"
    argument_class = HelpArguments
    attackmapping = []
    attributes = simple_command_attributes()

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        command = taskData.args.get_arg("command") or ""
        return PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
            DisplayParams=command,
        )

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
