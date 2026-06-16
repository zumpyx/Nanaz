import json
import shlex

from mythic_container.MythicCommandBase import *

from ._base import error_aware_process_response


class PtyArguments(TaskArguments):
    MIN_OUTPUT_CHUNK_BYTES = 8192
    MAX_OUTPUT_CHUNK_BYTES = 1048576
    DEFAULT_OUTPUT_CHUNK_BYTES = 65536

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="shell",
                cli_name="Shell",
                display_name="Shell",
                type=ParameterType.ChooseOne,
                choices=["default", "sh", "bash"],
                default_value="default",
                description="Interactive shell to start.",
                parameter_group_info=[ParameterGroupInfo(required=False, ui_position=0)],
            ),
            CommandParameter(
                name="output_chunk_bytes",
                cli_name="OutputChunkBytes",
                display_name="Output Chunk Bytes",
                type=ParameterType.Number,
                default_value=self.DEFAULT_OUTPUT_CHUNK_BYTES,
                description="Maximum bytes to aggregate into one interactive output record. Larger values reduce Mythic UI pagination for long shell logs.",
                parameter_group_info=[ParameterGroupInfo(required=False, ui_position=1)],
            ),
        ]

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)
        self._validate_args()

    async def parse_arguments(self):
        command_line = self.command_line.strip()
        if not command_line:
            self.set_arg("shell", "default")
            self.set_arg("output_chunk_bytes", self.DEFAULT_OUTPUT_CHUNK_BYTES)
            return

        if command_line.startswith("{"):
            self.load_args_from_dictionary(json.loads(command_line))
            self._validate_args()
            return

        tokens = shlex.split(command_line)
        if tokens and tokens[0].startswith(("-", "/")):
            i = 0
            while i < len(tokens):
                key = tokens[i].lower()
                if key in ("-shell", "/shell") and i + 1 < len(tokens):
                    self.set_arg("shell", tokens[i + 1].lower())
                    i += 2
                elif key in ("-outputchunkbytes", "/outputchunkbytes") and i + 1 < len(tokens):
                    self.set_arg("output_chunk_bytes", int(tokens[i + 1]))
                    i += 2
                else:
                    raise Exception(f"unknown pty argument: {tokens[i]}")
        else:
            self.set_arg("shell", tokens[0].lower())
            if len(tokens) > 1:
                self.set_arg("output_chunk_bytes", int(tokens[1]))
            if len(tokens) > 2:
                raise Exception("pty accepts at most: pty [shell] [output_chunk_bytes]")

        self._validate_args()

    def _validate_args(self):
        shell = (self.get_arg("shell") or "default").lower()
        if shell not in ("default", "sh", "bash"):
            raise Exception("pty shell must be one of: default, sh, bash.")
        self.set_arg("shell", shell)

        chunk_size = self.get_arg("output_chunk_bytes")
        if chunk_size in (None, ""):
            chunk_size = self.DEFAULT_OUTPUT_CHUNK_BYTES
        chunk_size = int(chunk_size)
        if chunk_size < self.MIN_OUTPUT_CHUNK_BYTES or chunk_size > self.MAX_OUTPUT_CHUNK_BYTES:
            raise Exception(
                f"output_chunk_bytes must be between {self.MIN_OUTPUT_CHUNK_BYTES} and {self.MAX_OUTPUT_CHUNK_BYTES}."
            )
        self.set_arg("output_chunk_bytes", chunk_size)


class PtyCommand(CommandBase):
    cmd = "pty"
    needs_admin = False
    help_cmd = "pty [sh|bash] [output_chunk_bytes]"
    description = "Start an interactive shell task backed by a Unix pseudo-terminal."
    version = 1
    author = "@zumpyx"
    argument_class = PtyArguments
    attackmapping = ["T1059"]
    supported_ui_features = ["task_response:interactive"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Linux],
        builtin=False,
        load_only=False,
        suggested_command=False,
    )

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        shell = taskData.args.get_arg("shell") or "default"
        output_chunk_bytes = (
            taskData.args.get_arg("output_chunk_bytes")
            or PtyArguments.DEFAULT_OUTPUT_CHUNK_BYTES
        )
        display = f"-Shell {shell} -OutputChunkBytes {output_chunk_bytes}"
        return PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
            DisplayParams=display,
        )

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)
