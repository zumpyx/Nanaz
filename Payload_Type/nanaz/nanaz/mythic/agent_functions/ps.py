from mythic_container.MythicCommandBase import *
from mythic_container.MythicGoRPC.send_mythic_rpc_task_update import (
    MythicRPCTaskUpdateMessage,
    SendMythicRPCTaskUpdate,
)


class PsArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        # `host` is intentionally NOT exposed as a CommandParameter.
        # Mythic's UI calls the process browser with `{host, ...}` and
        # the RPC carries the host for us; the CLI does not need to
        # type it. If a future feature really needs the operator to
        # supply a host string, the field belongs in the Rust
        # `Params` struct — exposing it here would clutter the
        # parameter panel for every ps invocation.
        self.args = []

    async def parse_dictionary(self, dictionary_arguments):
        # UI / process browser sends {host: ...}; we don't store it
        # here (Rust derives it from the callback), but the call
        # keeps the framework happy.
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        # CLI: no parameters. `ps` takes no args.
        if self.command_line.strip():
            raise Exception("ps takes no command line arguments.")


class PsCommand(CommandBase):
    cmd = "ps"
    needs_admin = False
    help_cmd = "ps"
    description = "List running processes. Integrates with Mythic's process browser UI."
    version = 1
    author = "@zumpyx"
    argument_class = PsArguments
    attackmapping = ["T1057"]
    supported_ui_features = ["process_browser:list"]
    browser_script = BrowserScript(
        script_name="ps_new", author="@zumpyx", for_new_ui=True
    )
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
        response.DisplayParams = "list processes"
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        """Format the structured `processes` payload as a one-shot
        text block for the operator's tasking pane.

        Notes on the doubled-output bug that was here before:

        Earlier versions called `SendMythicRPCTaskUpdate` from inside
        a loop *and* set `user_output` on the final response. The
        Mythic UI then rendered two output blocks for every `ps`
        call. The Rust side now never sets `user_output` (it returns
        the structured `processes` array only), so this function is
        the single writer.

        Error path: the Rust side sets `status: "error"` and
        `user_output: "..."` on failure. We surface that as a failed
        task with the operator-facing error string.
        """
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        if not isinstance(response, dict):
            return resp

        if response.get("status") == "error":
            err = response.get("user_output") or "ps failed"
            resp.Success = False
            resp.Error = err
            await SendMythicRPCTaskUpdate(
                MythicRPCTaskUpdateMessage(TaskID=task.Task.ID, UpdateStdout=err)
            )
            return resp

        procs = response.get("processes") or []
        if not procs:
            await SendMythicRPCTaskUpdate(
                MythicRPCTaskUpdateMessage(TaskID=task.Task.ID, UpdateStdout="no processes")
            )
            return resp

        lines = [f"{'PID':<8} {'PPID':<8} {'NAME':<24} CMDLINE"]
        lines.append("-" * 72)
        for p in procs[:40]:
            name = p.get("name", "")
            if len(name) > 24:
                name = name[:23] + "…"
            cmd = p.get("command_line") or "-"
            if len(cmd) > 60:
                cmd = cmd[:59] + "…"
            lines.append(
                f"{p.get('process_id', 0):<8} "
                f"{str(p.get('parent_process_id', '-')):<8} "
                f"{name:<24} "
                f"{cmd}"
            )
        if len(procs) > 40:
            lines.append(f"… and {len(procs) - 40} more")
        lines.append(f"── {len(procs)} processes ──")
        await SendMythicRPCTaskUpdate(
            MythicRPCTaskUpdateMessage(TaskID=task.Task.ID, UpdateStdout="\n".join(lines))
        )
        return resp
