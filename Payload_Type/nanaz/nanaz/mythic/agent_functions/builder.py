import os
import asyncio
import shutil
import json
import pathlib

from mythic_container.PayloadBuilder import *
from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *


class Nanaz(PayloadType):
    name = "nanaz"
    file_extension = "exe"
    agent_type: str = AgentType.Agent
    author = "@zumpyx"
    mythic_encrypts = True
    supported_os = [
        SupportedOS.Windows, SupportedOS.Linux
    ]
    semver = "0.1.0"
    wrapper = False
    wrapped_payloads = ["scarecrow_wrapper", "service_wrapper"]
    c2_profiles = ["http"]
    note = """
A fully featured rust compatible training agent. Version: {}.
    """.format(semver)
    supports_dynamic_loading = True
    supports_multiple_c2_instances_in_build = False
    supports_multiple_c2_in_build = False

    build_parameters = [
        BuildParameter(
            name="output_type",
            parameter_type=BuildParameterType.ChooseOne,
            choices=["WinExe", "Shellcode", "Service", "Source"],
            default_value="WinExe",
            description="Output as shellcode, executable, sourcecode, or service.",
            ui_position=1,
        ),
        BuildParameter(
            name="shellcode_format",
            parameter_type=BuildParameterType.ChooseOne,
            choices=["Binary", "Base64", "C", "Ruby", "Python", "Powershell", "C#", "Hex"],
            default_value="Binary",
            description="Donut shellcode format options.",
            group_name="Shellcode Options",
            hide_conditions=[
                HideCondition(name="output_type", operand=HideConditionOperand.NotEQ, value="Shellcode")
            ],
            ui_position=4
        ),
        BuildParameter(
            name="shellcode_bypass",
            parameter_type=BuildParameterType.ChooseOne,
            choices=["None", "Abort on fail", "Continue on fail"],
            default_value="Continue on fail",
            description="Donut shellcode AMSI/WLDP/ETW Bypass options.",
            group_name="Shellcode Options",
            hide_conditions=[
                HideCondition(name="output_type", operand=HideConditionOperand.NotEQ, value="Shellcode")
            ],
            ui_position=5
        ),
        BuildParameter(
            name="adjust_filename",
            parameter_type=BuildParameterType.Boolean,
            default_value=False,
            description="Automatically adjust payload extension based on selected choices.",
            ui_position=3,
        ),
        BuildParameter(
            name="debug",
            parameter_type=BuildParameterType.Boolean,
            default_value=False,
            description="Create a DEBUG version.",
            ui_position=2,
        ),
        BuildParameter(
            name="enable_keying",
            parameter_type=BuildParameterType.Boolean,
            default_value=False,
            description="Enable environmental keying to restrict agent execution to specific systems.",
            group_name="Keying Options",
        ),
        BuildParameter(
            name="keying_method",
            parameter_type=BuildParameterType.ChooseOne,
            choices=["Hostname", "Domain", "Registry"],
            default_value="Hostname",
            description="Method of environmental keying.",
            group_name="Keying Options",
            hide_conditions=[
                HideCondition(name="enable_keying", operand=HideConditionOperand.NotEQ, value=True)
            ]
        ),
        BuildParameter(
            name="keying_value",
            parameter_type=BuildParameterType.String,
            default_value="",
            description="The hostname or domain name the agent should match (case-insensitive). Agent will exit if it doesn't match.",
            group_name="Keying Options",
            hide_conditions=[
                HideCondition(name="enable_keying", operand=HideConditionOperand.NotEQ, value=True),
                HideCondition(name="keying_method", operand=HideConditionOperand.EQ, value="Registry")
            ]
        ),
        BuildParameter(
            name="registry_path",
            parameter_type=BuildParameterType.String,
            default_value="",
            description="Full registry path (e.g., HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ProductName)",
            group_name="Keying Options",
            hide_conditions=[
                HideCondition(name="enable_keying", operand=HideConditionOperand.NotEQ, value=True),
                HideCondition(name="keying_method", operand=HideConditionOperand.NotEQ, value="Registry")
            ]
        ),
        BuildParameter(
            name="registry_value",
            parameter_type=BuildParameterType.String,
            default_value="",
            description="The registry value to check against.",
            group_name="Keying Options",
            hide_conditions=[
                HideCondition(name="enable_keying", operand=HideConditionOperand.NotEQ, value=True),
                HideCondition(name="keying_method", operand=HideConditionOperand.NotEQ, value="Registry")
            ]
        ),
        BuildParameter(
            name="registry_comparison",
            parameter_type=BuildParameterType.ChooseOne,
            choices=["Matches", "Contains"],
            default_value="Matches",
            description="Matches (secure, hash-based) or Contains (WEAK, plaintext comparison). WARNING: Contains mode stores the value in plaintext!",
            group_name="Keying Options",
            hide_conditions=[
                HideCondition(name="enable_keying", operand=HideConditionOperand.NotEQ, value=True),
                HideCondition(name="keying_method", operand=HideConditionOperand.NotEQ, value="Registry")
            ]
        ),
    ]

    agent_path = pathlib.Path(".") / "nanaz" / "mythic"
    agent_code_path = pathlib.Path(".") / "nanaz" / "agent_code"
    agent_icon_path = agent_path / "agent_functions" / "nanaz.svg"

    build_steps = [
        BuildStep(step_name="Gathering Files", step_description="Parsing options and generating config.json"),
        BuildStep(step_name="Compiling", step_description="Compiling Rust source code via Cargo"),
        BuildStep(step_name="Processing Output", step_description="Handling final artifact transformation"),
        BuildStep(step_name="Finalizing", step_description="Adjusting payload details and completing build")
    ]

    async def build(self) -> BuildResponse:
        resp = BuildResponse(status=BuildStatus.Error)
        build_msg = "=== 开始构建 Rust Agent ===\n"

        try:
            # =================================================================
            # 阶段 1: Gathering Files
            # =================================================================
            await SendMythicRPCBuildStepUpdate(MythicRPCBuildStepUpdateMessage(
                PayloadUUID=self.uuid, StepName="Gathering Files", StepStatus="Running"
            ))

            output_type = self.get_parameter("output_type")
            shellcode_format = self.get_parameter("shellcode_format")
            adjust_filename_param = self.get_parameter("adjust_filename")
            debug_mode = self.get_parameter("debug")

            enable_keying = self.get_parameter("enable_keying")
            keying_method = self.get_parameter("keying_method")
            keying_value = self.get_parameter("keying_value")
            registry_path = self.get_parameter("registry_path")
            registry_value = self.get_parameter("registry_value")
            registry_comparison = self.get_parameter("registry_comparison")

            target_os = self.selected_os
            build_msg += f"[*] 目标操作系统: {target_os}\n"

            c2_config = {}
            for c2 in self.c2info:
                profile = c2.get_c2profile()
                profile_name = profile["name"]

                if profile_name == "http":
                    c2_config[profile_name] = {}
                    for key, val in c2.get_parameters_dict().items():
                        if key == "AESPSK":
                            c2_config[profile_name]["aes_psk"] = val.get("enc_key") if val.get("enc_key") is not None else ""
                        else:
                            c2_config[profile_name][key] = val

            rust_agent_config = {
                "c2_profiles": c2_config,
                "build_options": {
                    "output_type": output_type,
                    "debug": debug_mode,
                    "target_os": target_os
                },
                "keying": {
                    "enabled": enable_keying,
                    "method": keying_method,
                    "value": keying_value,
                    "registry_path": registry_path,
                    "registry_value": registry_value,
                    "registry_comparison": registry_comparison
                }
            }

            config_path = self.agent_code_path / "config.json"
            with open(config_path, "w", encoding="utf-8") as f:
                json.dump(rust_agent_config, f, indent=4)

            build_msg += f"[*] 成功写入配置文件: {config_path}\n"
            await SendMythicRPCBuildStepUpdate(MythicRPCBuildStepUpdateMessage(
                PayloadUUID=self.uuid, StepName="Gathering Files", StepStatus="Success"
            ))

            # =================================================================
            # 阶段 2: Compiling
            # =================================================================
            await SendMythicRPCBuildStepUpdate(MythicRPCBuildStepUpdateMessage(
                PayloadUUID=self.uuid, StepName="Compiling", StepStatus="Running"
            ))

            cargo_args = ["build"]
            if not debug_mode:
                cargo_args.append("-r")

            target_triple = None
            if target_os == "Windows":
                target_triple = "x86_64-pc-windows-gnu"
                cargo_args.extend(["--target", target_triple])
            elif target_os == "Linux":
                target_triple = "x86_64-unknown-linux-musl"
                cargo_args.extend(["--target", target_triple])

            build_msg += f"[*] 执行编译命令: cargo {' '.join(cargo_args)}\n"
            build_msg += f"[*] 编译工作目录: {self.agent_code_path.resolve()}\n"

            # 🔥 关键修改：将 stderr 重定向到 stdout，因为 Cargo 编译日志默认输出到 stderr
            process = await asyncio.create_subprocess_exec(
                "cargo", *cargo_args,
                cwd=str(self.agent_code_path),
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT
            )

            # 实时读取所有的输出
            stdout, _ = await process.communicate()
            cargo_output = stdout.decode('utf-8', errors='ignore')
            build_msg += f"\n--- Cargo 编译日志输出 ---\n{cargo_output}\n-------------------------\n"

            if process.returncode != 0:
                await SendMythicRPCBuildStepUpdate(MythicRPCBuildStepUpdateMessage(
                    PayloadUUID=self.uuid, StepName="Compiling", StepStatus="Error"
                ))
                raise Exception(f"Cargo 编译非正常退出，退出码: {process.returncode}")

            await SendMythicRPCBuildStepUpdate(MythicRPCBuildStepUpdateMessage(
                PayloadUUID=self.uuid, StepName="Compiling", StepStatus="Success"
            ))

            # =================================================================
            # 阶段 3: Processing Output
            # =================================================================
            await SendMythicRPCBuildStepUpdate(MythicRPCBuildStepUpdateMessage(
                PayloadUUID=self.uuid, StepName="Processing Output", StepStatus="Running"
            ))

            rust_binary_name = "nanaz"
            profile_dir = "debug" if debug_mode else "release"

            if target_triple:
                binary_path = self.agent_code_path / "target" / target_triple / profile_dir / rust_binary_name
            else:
                binary_path = self.agent_code_path / "target" / profile_dir / rust_binary_name

            if target_os == "Windows" and not binary_path.exists():
                binary_path = binary_path.with_suffix(".exe")

            build_msg += f"[*] 正在检索编译产物路径: {binary_path}\n"

            if not binary_path.exists():
                await SendMythicRPCBuildStepUpdate(MythicRPCBuildStepUpdateMessage(
                    PayloadUUID=self.uuid, StepName="Processing Output", StepStatus="Error"
                ))
                raise Exception(f"无法定位生成的二进制文件，请检查目标名称是否正确。预期路径: {binary_path}")

            if output_type == "Source":
                zip_out_base = self.agent_code_path / "source"
                shutil.make_archive(str(zip_out_base), 'zip', str(self.agent_code_path))
                zip_path = self.agent_code_path / "source.zip"
                with open(zip_path, "rb") as f:
                    resp.payload = f.read()
                try: os.remove(zip_path)
                except: pass
            else:
                with open(binary_path, "rb") as f:
                    resp.payload = f.read()

            await SendMythicRPCBuildStepUpdate(MythicRPCBuildStepUpdateMessage(
                PayloadUUID=self.uuid, StepName="Processing Output", StepStatus="Success"
            ))

            # =================================================================
            # 阶段 4: Finalizing
            # =================================================================
            await SendMythicRPCBuildStepUpdate(MythicRPCBuildStepUpdateMessage(
                PayloadUUID=self.uuid, StepName="Finalizing", StepStatus="Running"
            ))

            resp.updated_filename = adjust_file_name(
                filename=self.filename,
                shellcode_format=shellcode_format,
                output_type=output_type,
                adjust_filename=adjust_filename_param,
                selected_os=target_os
            )

            resp.status = BuildStatus.Success
            # 将完整的编译成功日志同时放入 message 传给 Mythic 页面展示
            resp.message = build_msg + f"\n[+] 构建成功! 目标环境: {target_os} ({profile_dir.upper()})"

            await SendMythicRPCBuildStepUpdate(MythicRPCBuildStepUpdateMessage(
                PayloadUUID=self.uuid, StepName="Finalizing", StepStatus="Success"
            ))

        except Exception as p:
            # 🔥 关键修改：失败时，不仅要把报错信息带上，前面收集到的 Cargo 日志也要塞进 error_message
            build_msg += f"\n[!] 编译流程异常中断: {str(p)}"
            resp.status = BuildStatus.Error
            resp.error_message = build_msg

        return resp

def adjust_file_name(filename, shellcode_format, output_type, adjust_filename, selected_os):
    if not adjust_filename:
        return filename
    filename_pieces = filename.split(".")
    original_filename = ".".join(filename_pieces[:-1])

    if output_type == "Source":
        return original_filename + ".zip"
    elif output_type == "Shellcode":
        if shellcode_format == "Binary": return original_filename + ".bin"
        elif shellcode_format == "Base64": return original_filename + ".txt"
        elif shellcode_format == "C": return original_filename + ".c"
        elif shellcode_format == "Python": return original_filename + ".py"
        elif shellcode_format == "Powershell": return original_filename + ".ps1"
        else: return original_filename + ".txt"
    else:
        # 针对可执行文件或服务，根据选择的 OS 决定后缀
        if selected_os == "Windows":
            return original_filename + ".exe"
        else:
            return original_filename  # Linux 载荷通常没有后缀
