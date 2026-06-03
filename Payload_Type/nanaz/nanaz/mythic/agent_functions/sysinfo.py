from mythic_container.MythicCommandBase import *


class SysinfoArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = []

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        if self.command_line.strip():
            raise Exception("sysinfo takes no arguments.")


class SysinfoCommand(CommandBase):
    cmd = "sysinfo"
    needs_admin = False
    help_cmd = "sysinfo"
    description = "Gather system information (OS, CPU, memory, uptime). Cross-platform."
    version = 1
    author = "@zumpyx"
    argument_class = SysinfoArguments
    attackmapping = ["T1082", "T1518"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows, SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=True,
    )

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        response.DisplayParams = "gather system info"
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        from ._base import error_aware_process_response
        return error_aware_process_response(task, response)
