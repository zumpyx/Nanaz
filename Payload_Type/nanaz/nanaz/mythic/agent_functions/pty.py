from mythic_container.MythicCommandBase import *

from ._base import error_aware_process_response


class PtyArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="shell",
                cli_name="Shell",
                display_name="Shell",
                type=ParameterType.ChooseOne,
                choices=["sh", "bash", "cmd", "powershell"],
                default_value="",
                description="Interactive shell to start. Empty uses the platform default.",
                parameter_group_info=[ParameterGroupInfo(required=False, ui_position=0)],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        shell = self.command_line.strip().lower()
        if shell:
            if shell not in ("sh", "bash", "cmd", "powershell"):
                raise Exception("pty shell must be one of: sh, bash, cmd, powershell.")
            self.set_arg("shell", shell)


class PtyCommand(CommandBase):
    cmd = "pty"
    needs_admin = False
    help_cmd = "pty [sh|bash|cmd|powershell]"
    description = "Start an interactive shell task."
    version = 1
    author = "@zumpyx"
    argument_class = PtyArguments
    attackmapping = ["T1059"]
    supported_ui_features = ["task_response:interactive", "shell", "execute:shell"]
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
        shell = taskData.args.get_arg("shell") or ""
        display = f"-Shell {shell}" if shell else "-Shell default"
        return PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
            DisplayParams=display,
        )

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
