from mythic_container.MythicCommandBase import *


class SleepArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="interval",
                type=ParameterType.Number,
                default_value=0,
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
                description="Jitter percentage (0–100). Extra sleep = interval * jitter% * random.",
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
            interval = int(parts[0])
        except Exception:
            raise Exception(
                "sleep requires an integer value (in seconds) to be passed on the command line to update the sleep value to."
            )
        if interval < 0:
            raise Exception("sleep interval must be >= 0 seconds.")
        self.set_arg("interval", interval)
        if len(parts) == 2:
            try:
                jitter = int(parts[1])
            except Exception:
                raise Exception(
                    "sleep requires an integer value for jitter, but received: {}".format(
                        parts[1]
                    )
                )
            if jitter < 0 or jitter > 100:
                raise Exception("sleep jitter must be between 0 and 100.")
            self.set_arg("jitter", jitter)


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
        suggested_command=True,
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
        if interval is None or interval < 0:
            response.Success = False
            response.Error = "sleep interval must be >= 0 seconds."
            return response
        if jitter is not None and (jitter < 0 or jitter > 100):
            response.Success = False
            response.Error = "sleep jitter must be between 0 and 100."
            return response
        displayParams = f"-interval {interval}"
        if jitter and jitter > 0:
            displayParams += f" -jitter {jitter}"
        response.DisplayParams = displayParams
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        from ._base import error_aware_process_response
        return error_aware_process_response(task, response)
