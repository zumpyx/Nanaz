from mythic_container.MythicCommandBase import *

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
    attributes = simple_command_attributes(suggested_command=True)

    async def create_go_tasking(self, taskData: PTTaskMessageAllData) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(TaskID=taskData.Task.ID, Success=True)
        path = taskData.args.get_arg("path") or "."
        response.DisplayParams = path

        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        if not isinstance(response, dict):
            return resp

        if response.get("status") == "error":
            err = response.get("user_output") or "ls failed"
            resp.Success = False
            resp.Error = err
        return resp
