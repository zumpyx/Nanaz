from mythic_container.MythicCommandBase import *
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
                parameter_group_info=[
                    ParameterGroupInfo(
                        required=False,
                        group_name="Default",
                        ui_position=1,
                    )
                ],
            ),
        ]


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
    browser_script = BrowserScript(
        script_name="ls_new", author="@zumpyx", for_new_ui=True
    )
    attributes = simple_command_attributes()

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        path = taskData.args.get_arg("path") or "."
        response.DisplayParams = path

        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        """Format the structured `file_browser` payload as a one-shot
        text block for the operator's tasking pane.

        Notes on the doubled-output bug that was here before:

        The Rust side used to set both `set_as_user_output: true` on
        the structured payload AND emit a separate `user_output`
        string. The Mythic UI in its new (file-browser) mode renders
        the structured payload itself, then ALSO shows the
        `user_output` text. Combined, this produced two output blocks
        for every `ls` call. The Rust side now leaves `user_output`
        empty and does NOT set `set_as_user_output`, so this function
        is the *only* writer of the human-readable table.

        The `success: Some(false)` case is treated specially — the
        agent has already written a useful error string into
        `user_output`; we re-emit it so it shows up in the tasking
        pane (the structured payload on its own is invisible to the
        operator when the command failed).
        """
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        if not isinstance(response, dict):
            return resp

        # Error case — surface the agent's own error string and mark
        # the task as failed. Without this the operator would see a
        # green tick next to a task that the file browser rendered as
        # an empty / missing directory.
        if response.get("status") == "error":
            err = response.get("user_output") or "ls failed"
            resp.Success = False
            resp.Error = err
            await SendMythicRPCTaskUpdate(
                MythicRPCTaskUpdateMessage(TaskID=task.Task.ID, UpdateStdout=err)
            )
            return resp

        fb = response.get("file_browser")
        if not fb:
            return resp

        # Single-file listing: just confirm the path, don't fake a table.
        if fb.get("is_file"):
            size = fb.get("size", 0) or 0
            output = join_display(fb.get('parent_path', ''), fb.get('name', ''))
            output = f"{output} ({format_size(size)})"
            await SendMythicRPCTaskUpdate(
                MythicRPCTaskUpdateMessage(TaskID=task.Task.ID, UpdateStdout=output)
            )
            return resp

        files = fb.get("files") or []
        if not files:
            output = f"empty: {join_display(fb.get('parent_path', ''), fb.get('name', ''))}"
            await SendMythicRPCTaskUpdate(
                MythicRPCTaskUpdateMessage(TaskID=task.Task.ID, UpdateStdout=output)
            )
            return resp

        lines = []
        for f in files:
            icon = "DIR " if not f.get("is_file") else "FILE"
            sz = f.get("size") or 0
            lines.append(
                f"  {icon}  {f.get('name', ''):<40}  {format_size(sz):>8}"
            )
        output = f"Listing: {join_display(fb.get('parent_path', ''), fb.get('name', ''))}\n"
        output += "\n".join(lines)
        output += f"\n── {len(files)} entries ──"
        await SendMythicRPCTaskUpdate(
            MythicRPCTaskUpdateMessage(TaskID=task.Task.ID, UpdateStdout=output)
        )
        return resp


def format_size(n: int) -> str:
    """Render a byte count as a human-readable size string."""
    if n < 1024:
        return f"{n}B"
    if n < 1024 * 1024:
        return f"{n // 1024}KB"
    if n < 1024 * 1024 * 1024:
        return f"{n // (1024 * 1024)}MB"
    return f"{n // (1024 * 1024 * 1024)}GB"


def join_display(parent: str, name: str) -> str:
    if not parent:
        return name or ""
    if not name:
        return parent
    sep = "\\" if "\\" in parent or (len(parent) >= 2 and parent[1] == ":") else "/"
    if parent.endswith(("/", "\\")):
        return parent + name
    return parent + sep + name
