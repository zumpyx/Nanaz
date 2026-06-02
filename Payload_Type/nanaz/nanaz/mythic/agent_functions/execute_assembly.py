import base64

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *

from ._base import error_aware_process_response, split_cli_preserve_backslashes


class ExecuteAssemblyArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="assembly_name",
                cli_name="Assembly",
                display_name="Assembly",
                type=ParameterType.ChooseOne,
                dynamic_query_function=self.get_files,
                description="Registered .NET assembly to execute.",
                parameter_group_info=[
                    ParameterGroupInfo(
                        required=True,
                        group_name="Default",
                        ui_position=1,
                    )
                ],
            ),
            CommandParameter(
                name="assembly_file",
                display_name="New Assembly",
                type=ParameterType.File,
                description="Upload a new .NET assembly to execute.",
                parameter_group_info=[
                    ParameterGroupInfo(
                        required=True,
                        group_name="New Assembly",
                        ui_position=1,
                    )
                ],
            ),
            CommandParameter(
                name="assembly_arguments",
                cli_name="Arguments",
                display_name="Arguments",
                type=ParameterType.String,
                default_value="",
                description="Arguments to pass to the assembly entry point.",
                parameter_group_info=[
                    ParameterGroupInfo(
                        required=False,
                        group_name="Default",
                        ui_position=2,
                    ),
                    ParameterGroupInfo(
                        required=False,
                        group_name="New Assembly",
                        ui_position=2,
                    ),
                ],
            ),
            CommandParameter(
                name="patch_exit",
                type=ParameterType.Boolean,
                default_value=True,
                description="Patch System.Environment.Exit so the assembly cannot terminate the agent.",
                parameter_group_info=[
                    ParameterGroupInfo(
                        required=False,
                        group_name="Default",
                        ui_position=3,
                    ),
                    ParameterGroupInfo(
                        required=False,
                        group_name="New Assembly",
                        ui_position=3,
                    ),
                ],
            ),
        ]

    async def get_files(
        self, inputMsg: PTRPCDynamicQueryFunctionMessage
    ) -> PTRPCDynamicQueryFunctionMessageResponse:
        response = PTRPCDynamicQueryFunctionMessageResponse(Success=False)
        file_resp = await SendMythicRPCFileSearch(
            MythicRPCFileSearchMessage(
                CallbackID=inputMsg.Callback,
                LimitByCallback=False,
                Filename="",
            )
        )
        if not file_resp.Success:
            response.Error = file_resp.Error
            return response

        choices = []
        for f in file_resp.Files:
            if f.Filename not in choices and f.Filename.lower().endswith(".exe"):
                choices.append(f.Filename)
        response.Success = True
        response.Choices = choices
        return response

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

    async def parse_arguments(self):
        if not self.command_line.strip():
            raise Exception(
                f"Require an assembly to execute.\n\tUsage: {ExecuteAssemblyCommand.help_cmd}"
            )
        if self.command_line.strip().startswith("{"):
            self.load_args_from_json_string(self.command_line)
            return

        tokens = split_cli_preserve_backslashes(self.command_line)
        assembly_name = ""
        assembly_arguments = ""
        patch_exit = None

        i = 0
        while i < len(tokens):
            token = tokens[i]
            lower = token.lower()
            if lower in ("-assembly", "/assembly"):
                i += 1
                if i >= len(tokens):
                    raise Exception("-Assembly requires a filename")
                assembly_name = tokens[i]
            elif lower in ("-arguments", "/arguments", "-args", "/args"):
                assembly_arguments = " ".join(tokens[i + 1 :])
                break
            elif lower in ("-patchexit", "/patchexit"):
                i += 1
                if i >= len(tokens):
                    raise Exception("-PatchExit requires true or false")
                patch_exit = tokens[i].lower() not in ("false", "0", "no")
            elif not assembly_name:
                assembly_name = token
            elif not assembly_arguments:
                assembly_arguments = " ".join(tokens[i:])
                break
            i += 1

        if not assembly_name:
            raise Exception(
                f"Require an assembly to execute.\n\tUsage: {ExecuteAssemblyCommand.help_cmd}"
            )

        self.add_arg("assembly_name", assembly_name)
        self.add_arg("assembly_arguments", assembly_arguments)
        if patch_exit is not None:
            self.set_arg("patch_exit", patch_exit)


class ExecuteAssemblyCommand(CommandBase):
    cmd = "execute_assembly"
    needs_admin = False
    help_cmd = "execute_assembly [Assembly.exe] [args]"
    description = "Execute a .NET assembly in-process using rustclr."
    version = 1
    author = "@zumpyx"
    argument_class = ExecuteAssemblyArguments
    attackmapping = ["T1059"]
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows],
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

        group_name = taskData.args.get_parameter_group_name()
        assembly_args = taskData.args.get_arg("assembly_arguments") or ""

        try:
            if group_name == "New Assembly":
                file_id = taskData.args.get_arg("assembly_file")
                file_search = await SendMythicRPCFileSearch(
                    MythicRPCFileSearchMessage(
                        TaskID=taskData.Task.ID,
                        AgentFileID=file_id,
                    )
                )
                if not file_search.Success or len(file_search.Files) == 0:
                    raise Exception(file_search.Error or "uploaded assembly not found")
                assembly_name = file_search.Files[0].Filename
                taskData.args.add_arg("assembly_name", assembly_name)
                taskData.args.remove_arg("assembly_file")
            else:
                assembly_name = taskData.args.get_arg("assembly_name")
                file_search = await SendMythicRPCFileSearch(
                    MythicRPCFileSearchMessage(
                        TaskID=taskData.Task.ID,
                        Filename=assembly_name,
                        MaxResults=1,
                    )
                )
                if not file_search.Success or len(file_search.Files) == 0:
                    raise Exception(file_search.Error or f"assembly not found: {assembly_name}")
                file_id = file_search.Files[0].AgentFileId

            content_resp = await SendMythicRPCFileGetContent(
                MythicRPCFileGetContentMessage(file_id)
            )
            if not content_resp.Success or content_resp.Content is None:
                raise Exception(content_resp.Error or "failed to fetch assembly bytes")

            taskData.args.add_arg(
                "assembly_b64",
                base64.b64encode(content_resp.Content).decode("utf-8"),
            )
            taskData.args.remove_arg("assembly_name")
            response.DisplayParams = f"-Assembly {assembly_name}"
            if assembly_args:
                response.DisplayParams += f" -Arguments {assembly_args}"
        except Exception as e:
            response.Success = False
            response.Error = str(e)

        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        return error_aware_process_response(task, response)


class ExecuteAssemblyCamelCommand(ExecuteAssemblyCommand):
    cmd = "executeAssembly"
    help_cmd = "executeAssembly [Assembly.exe] [args]"
    attributes = CommandAttributes(
        spawn_and_injectable=False,
        supported_os=[SupportedOS.Windows],
        builtin=False,
        load_only=False,
        suggested_command=False,
        alias=True,
    )

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        response = await super().create_go_tasking(taskData)
        response.CommandName = ExecuteAssemblyCommand.cmd
        return response
