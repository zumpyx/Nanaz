from mythic_container.MythicCommandBase import *


class SleepArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="interval",
                type=ParameterType.Number,
                default_value=-1,
                parameter_group_info=[
                    ParameterGroupInfo(
                        ui_position=0,
                        required=True,
                    )
                ],
            ),
            CommandParameter(
                name="jitter",
                type=ParameterType.Number,
                default_value=0,
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
            raise Exception(
                "sleep requires an integer value (in seconds) to be passed on the command line to update the sleep value to."
            )
        parts = self.command_line.split(" ", maxsplit=1)
        try:
            self.set_arg("interval", int(parts[0]))
        except Exception:
            raise Exception(
                "sleep requires an integer value (in seconds) to be passed on the command line to update the sleep value to."
            )
        if len(parts) == 2:
            try:
                self.set_arg("jitter", int(parts[1]))
            except Exception:
                raise Exception(
                    "sleep requires an integer value for jitter, but received: {}".format(
                        parts[1]
                    )
                )


class SleepCommand(CommandBase):
    cmd = "sleep"
    needs_admin = False
    help_cmd = "sleep [seconds] [jitter]"
    description = "Change the implant's sleep interval and optional jitter percentage."
    version = 1
    author = "@zumpyx"
    argument_class = SleepArguments
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
        interval = taskData.args.get_arg("interval")
        jitter = taskData.args.get_arg("jitter")
        displayParams = f"-interval {interval}"
        if jitter and jitter > 0:
            displayParams += f" -jitter {jitter}"
        response.DisplayParams = displayParams
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        return resp
