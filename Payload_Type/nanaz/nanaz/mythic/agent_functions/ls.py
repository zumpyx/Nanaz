import json

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
from mythic_container.MythicGoRPC.send_mythic_rpc_task_update import (
    MythicRPCTaskUpdateMessage,
    SendMythicRPCTaskUpdate,
)

from ._base import FileBrowserArguments, simple_command_attributes


class LsArguments(FileBrowserArguments):
    cli_takes_path = True
    command_name = "ls"

    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="path",
                type=ParameterType.String,
                default_value=".",
            ),
            CommandParameter(
                name="recursive",
                type=ParameterType.Boolean,
                default_value=False,
            ),
            CommandParameter(
                name="host",
                type=ParameterType.String,
                default_value="",
            ),
        ]

    async def parse_arguments(self):
        # Extend FileBrowserArguments.parse_arguments to also recognise
        # the `ls -r` / `ls -r /path` CLI forms.
        cl = self.command_line.strip()
        if not cl or cl.startswith("{"):
            await super().parse_arguments()
            return
        path = "."
        recursive = False
        for part in cl.split():
            if part in ("-r", "-R"):
                recursive = True
            else:
                path = part
        self.set_arg("path", path)
        self.set_arg("recursive", recursive)


class LsCommand(CommandBase):
    cmd = "ls"
    needs_admin = False
    help_cmd = "ls [path]"
    description = "List files and directories. Integrates with Mythic file browser UI."
    version = 1
    author = "@zumpyx"
    argument_class = LsArguments
    attackmapping = ["T1083", "T1105"]
    supported_ui_features = ["file_browser:list"]
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        path = taskData.args.get_arg("path")
        rec = taskData.args.get_arg("recursive")
        response.DisplayParams = f"{path}" + (" -r" if rec else "")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        if isinstance(response, dict):
            fb = response.get("file_browser")
            if fb and fb.get("files"):
                lines = []
                for f in fb["files"]:
                    icon = "DIR " if not f.get("is_file") else "FILE"
                    sz = f.get("size") or 0
                    if sz < 1024:
                        size_str = f"{sz}B"
                    elif sz < 1048576:
                        size_str = f"{sz//1024}KB"
                    else:
                        size_str = f"{sz//1048576}MB"
                    lines.append(f"  {icon}  {f.get('name', ''):<40}  {size_str:>8}")
                output = f"Listing: {fb.get('parent_path', '')}/{fb.get('name', '')}\n"
                output += "\n".join(lines)
                output += f"\n── {len(fb['files'])} entries ──"
                await SendMythicRPCTaskUpdate(
                    MythicRPCTaskUpdateMessage(TaskID=task.Task.ID, UpdateStdout=output)
                )
        return resp
