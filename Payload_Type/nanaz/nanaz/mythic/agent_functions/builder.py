import os
import asyncio
import shutil
import json
import pathlib
import traceback

from mythic_container.PayloadBuilder import *
from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *

def adjust_file_name(filename: str, shellcode_format: str, output_type: str, adjust_filename: bool, selected_os: str) -> str:
    if not adjust_filename:
        return filename

    # 使用 pathlib 优雅地获取不带后缀的文件名
    pure_name = pathlib.Path(filename).stem

    if output_type == "Source":
        return f"{pure_name}.zip"

    if output_type == "Shellcode":
        # 使用字典映射替代 if-elif 链
        format_map = {
            "Binary": ".bin",
            "Base64": ".txt",
            "C": ".c",
            "Python": ".py",
            "Powershell": ".ps1"
        }
        ext = format_map.get(shellcode_format, ".txt")
        return f"{pure_name}{ext}"

    if "windows" in str(selected_os).lower():
        return f"{pure_name}.exe"

    return pure_name


async def _update_step(payload_uuid: str, step_name: str, success: bool = True):
    """独立的辅助函数，用于统一管理并安全更新 Mythic 构建步骤状态"""
    try:
        await SendMythicRPCPayloadUpdatebuildStep(
            MythicRPCPayloadUpdateBuildStepMessage(
                PayloadUUID=payload_uuid,
                StepName=step_name,
                StepSuccess=success
            )
        )
    except Exception as rpc_err:
        print(f"[-] [RPC 错误] 无法更新 '{step_name}' 状态: {rpc_err}", flush=True)


class Nanaz(PayloadType):
    name = "nanaz"
    file_extension = "exe"
    agent_type: str = AgentType.Agent
    author = "@zumpyx"
    mythic_encrypts = True
    supported_os = [SupportedOS.Windows, SupportedOS.Linux]
    semver = "0.1.0"
    wrapper = False
    wrapped_payloads = []
    c2_profiles = ["http"]
    note = f"A fully featured rust compatible training agent. Version: {semver}."
    supports_dynamic_loading = True
    supports_multiple_c2_instances_in_build = True
    supports_multiple_c2_in_build = True

    # 已移除无用的 enable_keying 参数，前端界面更干净
    build_parameters = [
        BuildParameter(
            name="output_type",
            parameter_type=BuildParameterType.ChooseOne,
            choices=["WinExe", "Shellcode", "Service", "Source"],
            default_value="WinExe",
            description="Output as shellcode, executable, sourcecode, or service."
        ),
        BuildParameter(
            name="shellcode_format",
            parameter_type=BuildParameterType.ChooseOne,
            choices=["Binary", "Base64", "C", "Python", "Powershell"],
            default_value="Binary",
            description="If outputting as shellcode, select the format."
        ),
        BuildParameter(
            name="debug",
            parameter_type=BuildParameterType.Boolean,
            default_value=False,
            description="Create a DEBUG version."
        ),
        BuildParameter(
            name="adjust_filename",
            parameter_type=BuildParameterType.Boolean,
            default_value=False,
            description="Automatically adjust payload extension based on selected choices."
        ),
    ]

    agent_path = pathlib.Path(".") / "nanaz" / "mythic"
    agent_code_path = pathlib.Path(".") / "nanaz" / "agent_code"
    agent_icon_path = pathlib.Path(".") / "nanaz" / "nanaz.svg"

    build_steps = [
        BuildStep(step_name="Gathering Files", step_description="Parsing options and generating config.json"),
        BuildStep(step_name="Compiling", step_description="Compiling Rust source code via Cargo"),
        BuildStep(step_name="Processing Output", step_description="Handling final artifact transformation"),
        BuildStep(step_name="Finalizing", step_description="Adjusting payload details and completing build")
    ]

    async def build(self) -> BuildResponse:
        resp = BuildResponse(status=BuildStatus.Error)
        build_msg = "=== 开始构建 Rust Agent ===\n"

        print(f"\n[+] 收到 Mythic 构建请求 (UUID: {self.uuid})，开始执行...", flush=True)

        try:
            # =================================================================
            # 阶段 1: Gathering Files
            # =================================================================
            output_type = self.get_parameter("output_type")
            shellcode_format = self.get_parameter("shellcode_format")
            adjust_filename_param = self.get_parameter("adjust_filename")
            debug_mode = self.get_parameter("debug")

            target_os_raw = getattr(self, "selected_os", "Linux")
            target_os = "Windows" if "windows" in str(target_os_raw).lower() else "Linux"

            status_info = f"[*] 目标系统: {target_os} | 模式: {'DEBUG' if debug_mode else 'RELEASE'}"
            build_msg += status_info + "\n"
            print(f"[+] {status_info}", flush=True)

            # 修复：将列表重构为纯字典（Map）结构，生成 "c2_profiles": { "http": {...} }
            c2_profiles_map = {}
            for c2 in self.c2info:
                profile = c2.get_c2profile()
                profile_name = profile["name"]

                if profile_name == "http":
                    profile_params = {}
                    for key, val in c2.get_parameters_dict().items():
                        if key == "AESPSK":
                            profile_params["aes_psk"] = val.get("enc_key") if val is not None else ""
                        else:
                            profile_params[key] = val

                    # 用 profile 名称作为 Key 存入字典
                    c2_profiles_map[profile_name] = profile_params

            rust_agent_config = {
                "payload_uuid": self.uuid,
                "c2_profiles": c2_profiles_map,
                "build_options": {
                    "output_type": output_type,
                    "debug": debug_mode,
                    "target_os": target_os
                },
            }

            self.agent_code_path.mkdir(parents=True, exist_ok=True)
            config_path = self.agent_code_path / "config.json"
            with open(config_path, "w", encoding="utf-8") as f:
                json.dump(rust_agent_config, f, indent=4)

            build_msg += f"[*] 成功写入配置文件: {config_path}\n"
            print(f"[+] 配置文件已写入: {config_path}", flush=True)

            # 本阶段任务圆满结束，更新一次状态即可
            await _update_step(self.uuid, "Gathering Files", True)

            # =================================================================
            # 阶段 2: Compiling
            # =================================================================
            cargo_args = ["build"]
            if not debug_mode:
                cargo_args.append("-r")

            target_triple = None
            if target_os == "Windows":
                target_triple = "x86_64-pc-windows-gnu"
            elif target_os == "Linux":
                target_triple = "x86_64-unknown-linux-musl"

            if target_triple:
                cargo_args.extend(["--target", target_triple])

            build_msg += f"[*] 执行编译命令: cargo {' '.join(cargo_args)}\n"
            print(f"[+] 启动工作目录: {self.agent_code_path.resolve()}", flush=True)
            print(f"[+] 运行命令: cargo {' '.join(cargo_args)}", flush=True)

            process = await asyncio.create_subprocess_exec(
                "cargo", *cargo_args,
                cwd=str(self.agent_code_path),
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT
            )

            build_msg += "\n--- Cargo 实时编译日志 ---\n"
            print("[*] ---- Cargo 编译输出开始 ----", flush=True)

            while True:
                line = await process.stdout.readline()
                if not line:
                    break
                decoded_line = line.decode('utf-8', errors='ignore')
                print(f"    {decoded_line.strip()}", flush=True)
                build_msg += decoded_line

            print("[*] ---- Cargo 编译输出结束 ----", flush=True)
            await process.wait()

            if process.returncode != 0:
                await _update_step(self.uuid, "Compiling", False)
                raise Exception(f"Cargo 编译非正常退出，退出码: {process.returncode}")

            await _update_step(self.uuid, "Compiling", True)

            # =================================================================
            # 阶段 3: Processing Output
            # =================================================================
            rust_binary_name = "nanaz"
            profile_dir = "debug" if debug_mode else "release"

            if target_triple:
                binary_path = self.agent_code_path / "target" / target_triple / profile_dir / rust_binary_name
            else:
                binary_path = self.agent_code_path / "target" / profile_dir / rust_binary_name

            if target_os == "Windows" and not binary_path.exists():
                binary_path = binary_path.with_suffix(".exe")

            build_msg += f"[*] 正在检索编译产物路径: {binary_path}\n"
            print(f"[+] 正在检查产物: {binary_path}", flush=True)

            if not binary_path.exists():
                await _update_step(self.uuid, "Processing Output", False)
                raise Exception(f"无法定位生成的二进制文件。预期路径: {binary_path}")

            if output_type == "Source":
                zip_out_base = self.agent_code_path / "source"
                shutil.make_archive(str(zip_out_base), 'zip', str(self.agent_code_path))
                zip_path = self.agent_code_path / "source.zip"
                with open(zip_path, "rb") as f:
                    resp.payload = f.read()
                try:
                    zip_path.unlink()
                except Exception:
                    pass
            else:
                with open(binary_path, "rb") as f:
                    resp.payload = f.read()

            await _update_step(self.uuid, "Processing Output", True)

            # =================================================================
            # 阶段 4: Finalizing
            # =================================================================
            resp.updated_filename = adjust_file_name(
                filename=self.filename,
                shellcode_format=shellcode_format,
                output_type=output_type,
                adjust_filename=adjust_filename_param,
                selected_os=target_os
            )

            resp.status = BuildStatus.Success
            resp.build_message = build_msg + f"\n[+] 构建成功! 目标环境: {target_os} ({profile_dir.upper()})"
            print("[+] 构建全流程顺利结束！", flush=True)

            await _update_step(self.uuid, "Finalizing", True)

        except Exception as p:
            err_msg = f"\n[!] 编译流程异常中断: {str(p)}\n{traceback.format_exc()}"
            build_msg += err_msg
            print(f"[-] 异常崩溃: {err_msg}", flush=True)
            resp.status = BuildStatus.Error
            resp.build_message = build_msg

        return resp
