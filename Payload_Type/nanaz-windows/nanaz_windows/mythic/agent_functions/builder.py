import asyncio
import json
import os
import pathlib
import shutil
import tempfile
import traceback
from pathlib import Path
from typing import Any, cast

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
from mythic_container.PayloadBuilder import *

TARGET = "x86_64-pc-windows-gnu"

COMMON_COMMANDS = [
    "cat",
    "cd",
    "cp",
    "download",
    "drives",
    "env",
    "execute",
    "exit",
    "kill",
    "ls",
    "tree",
    "mkdir",
    "mv",
    "netstat",
    "ps",
    "pwd",
    "resolve",
    "rm",
    "rpfwd",
    "sleep",
    "socks",
    "sysinfo",
    "upload",
    "wget",
    "whoami",
]

DEFAULT_COMMANDS = [
    "cat",
    "cd",
    "download",
    "drives",
    "env",
    "exit",
    "kill",
    "ls",
    "mkdir",
    "mv",
    "netstat",
    "ps",
    "pwd",
    "resolve",
    "rm",
    "sleep",
    "sysinfo",
    "upload",
    "whoami",
]

WINDOWS_COMMANDS = COMMON_COMMANDS + [
    "cmd",
    "execute_assembly",
    "powerpick",
    "powershell",
]

AGENT_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
MYTHIC_PATH = AGENT_ROOT / "mythic"
AGENT_CODE_PATH = AGENT_ROOT / "agent_code"

TOOL_DIRS = [
    pathlib.Path("/root/.cargo/bin"),
    pathlib.Path("/usr/local/cargo/bin"),
    pathlib.Path("/usr/local/bin"),
    pathlib.Path("/usr/bin"),
    pathlib.Path("/bin"),
]


def _resolve_tool(name: str) -> str:
    found = shutil.which(name)
    if found:
        return found
    for directory in TOOL_DIRS:
        candidate = directory / name
        if candidate.exists() and os.access(candidate, os.X_OK):
            return str(candidate)
    searched = os.environ.get("PATH", "")
    extra = ":".join(str(path) for path in TOOL_DIRS)
    raise FileNotFoundError(
        f"required build tool '{name}' not found; searched PATH={searched} and {extra}"
    )


def _build_env() -> dict[str, str]:
    env = os.environ.copy()
    path_entries = [str(path) for path in TOOL_DIRS]
    if env.get("PATH"):
        path_entries.append(env["PATH"])
    env["PATH"] = os.pathsep.join(dict.fromkeys(path_entries))
    return env


def _read_cargo_semver() -> str:
    cargo_toml = AGENT_CODE_PATH / "Cargo.toml"
    try:
        text = cargo_toml.read_text(encoding="utf-8")
    except OSError:
        return "0.0.0"

    in_package = False
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("["):
            in_package = line == "[package]"
            continue
        if not in_package:
            continue
        if line.startswith("version") and "=" in line:
            _, _, value = line.partition("=")
            value = value.strip().strip('"').strip("'")
            if value:
                return value
    return "0.0.0"


def _extract_aes_psk(value):
    if isinstance(value, dict):
        key = value.get("enc_key")
    else:
        key = value
    if key is None:
        return None
    key = str(key).strip()
    return key or None


def _commands_for_windows(requested) -> tuple[list[str], list[str]]:
    requested = list(requested or [])
    if not requested:
        return [command for command in DEFAULT_COMMANDS if command in WINDOWS_COMMANDS], []

    requested_set = set(requested)
    selected = [command for command in WINDOWS_COMMANDS if command in requested_set]
    dropped = sorted(command for command in requested if command not in WINDOWS_COMMANDS)
    return selected, dropped


async def _update_build_step(
    payload_uuid: str,
    step_name: str,
    stdout: str,
    stderr: str = "",
    success: bool = True,
) -> None:
    await SendMythicRPCPayloadUpdatebuildStep(
        MythicRPCPayloadUpdateBuildStepMessage(
            PayloadUUID=payload_uuid,
            StepName=step_name,
            StepStdout=stdout,
            StepStderr=stderr,
            StepSuccess=success,
        )
    )


class NanazWindows(PayloadType):
    name = "nanaz-windows"
    file_extension = "exe"
    author = "@zumpyx"
    mythic_encrypts = True
    supported_os = [SupportedOS.Windows]
    semver = _read_cargo_semver()
    wrapped_payloads = []
    note = "A rust+zig compatible training agent. Version: {}.".format(semver)
    supports_dynamic_loading = True
    supports_multiple_c2_instances_in_build = False
    supports_multiple_c2_in_build = False
    c2_profiles = ["http"]

    output_arch_options = [
        "x86_64-pc-windows-gnu",
        # TODO: enable when the Rust crate and builder support these targets.
        # "aarch64-pc-windows-gnullvm",
        # "i686-pc-windows-gnullvm",
        # "x86_64-pc-windows-gnullvm",
    ]
    output_type_options = [
        "Portable Executable",
        # TODO: enable once packaging exists.
        # "Dynamic Link Library",
        # "Service Executable",
        # "Shellcode",
    ]
    # TODO: wire these into Cargo features/build.rs before exposing in Mythic.
    # runtime_sandbox_options = [
    #     "require_user_interaction",
    #     "startup_delay_1-5min",
    #     "startup_delay_10-20min",
    #     "check_network_dns_http",
    #     "check_time_progression",
    #     "check_installed_programs",
    #     "check_hardware_configuration",
    #     "check_virtualization_environment",
    # ]
    # runtime_syscall_options = [
    #     "Standard API",
    #     "Direct Syscalls",
    #     "Indirect Syscalls",
    # ]
    # runtime_sleepmask_options = [
    #     "Ekko",
    #     "TpSetTimer",
    #     "TpSetWait",
    #     "APC",
    # ]
    # spoof_process_options = {
    #     "x86": "C:/Windows/System32/notepad.exe",
    #     "x64": "C:/Windows/System32/notepad.exe",
    #     "arm": "C:/Windows/System32/notepad.exe",
    # }
    # stack_spoofing_options = ["Synthetic", "Unwind", "ProxyCalls"]
    # Future C2 profiles can be exposed here after src/c2 implements them:
    # c2_profiles = ["smb", "tcp", "http", "httpx", "websocket"]

    build_parameters = [
        BuildParameter(
            name="output_arch",
            parameter_type=BuildParameterType.ChooseOne,
            choices=output_arch_options,
            default_value="x86_64-pc-windows-gnu",
            description="Windows target triple.",
        ),
        BuildParameter(
            name="output_type",
            parameter_type=BuildParameterType.ChooseOne,
            choices=output_type_options,
            default_value="Portable Executable",
            description="Only Portable Executable is currently implemented.",
        ),
        # BuildParameter(
        #     name="runtime_sandbox",
        #     parameter_type=BuildParameterType.ChooseMultiple,
        #     choices=runtime_sandbox_options,
        #     default_value=[],
        #     description="Not implemented yet.",
        # ),
        # BuildParameter(
        #     name="stack_spoofing",
        #     parameter_type=BuildParameterType.ChooseOne,
        #     choices=stack_spoofing_options,
        #     default_value="Synthetic",
        #     description="Not implemented yet.",
        # ),
        # BuildParameter(
        #     name="block_dlls",
        #     parameter_type=BuildParameterType.Boolean,
        #     default_value=True,
        #     description="Not implemented yet.",
        # ),
        BuildParameter(
            name="debug",
            parameter_type=BuildParameterType.Boolean,
            default_value=False,
            description="Create a DEBUG version.",
        ),
    ]

    agent_path = cast(Any, MYTHIC_PATH)
    agent_code_path = cast(Any, AGENT_CODE_PATH)
    agent_func_path = cast(Any, MYTHIC_PATH / "agent_functions")
    agent_icon_path = cast(Any, AGENT_ROOT / "nanaz.svg")

    build_steps = [
        BuildStep(
            step_name="Initialize Configuration",
            step_description="Parse Mythic build parameters and selected commands, map build-time options, and generate the agent config.",
        ),
        BuildStep(
            step_name="Pre-Build Verification",
            step_description="Validate parameter interdependencies and enforce currently supported Windows build constraints.",
        ),
        # BuildStep(
        #     step_name="Resolve Dependencies",
        #     step_description="Provision a concurrency-safe compilation target directory and resolve upstream Cargo crates and toolchain requirements.",
        # ),
        BuildStep(
            step_name="Execute Compilation",
            step_description="Invoke Cargo with the Zig cross-linker for asynchronous, non-blocking binary synthesis.",
        ),
        # BuildStep(
        #     step_name="Strip Debug Symbols",
        #     step_description="Not implemented yet; release builds are already stripped by the Rust profile.",
        # ),
        BuildStep(
            step_name="Package & Deliver Artifact",
            step_description="Apply naming conventions and stream the artifact back to the Mythic controller.",
        ),
        BuildStep(
            step_name="Workspace Cleanup",
            step_description="Prune temporary build workspace data.",
        ),
    ]

    async def build(self) -> BuildResponse:
        resp = BuildResponse(status=cast(Any, BuildStatus.Error))
        stdout = ""
        stderr = ""
        build_root: Path | None = None

        try:
            stdout += "[*] Executing Step 1: Initialize Configuration...\n"

            selected_os = str(getattr(self, "selected_os", "")).lower()
            if selected_os and "windows" not in selected_os:
                raise ValueError(
                    f"unsupported selected_os '{selected_os}'; nanaz-windows only builds Windows payloads"
                )

            target = self.get_parameter("output_arch")
            if target != TARGET:
                raise ValueError(f"unsupported output_arch '{target}'")

            output_type = self.get_parameter("output_type")
            if output_type != "Portable Executable":
                raise ValueError(f"unsupported output_type '{output_type}'")

            debug = bool(self.get_parameter("debug"))
            selected_commands, dropped_commands = _commands_for_windows(
                self.commands.get_commands() if self.commands else []
            )
            resp.updated_command_list = selected_commands
            stdout += f"[+] Selected {len(selected_commands)} Windows command(s).\n"
            if dropped_commands:
                stdout += (
                    "[!] Dropped unsupported command(s): "
                    + ", ".join(dropped_commands)
                    + "\n"
                )

            if len(self.c2info) != 1:
                raise ValueError("nanaz-windows currently supports exactly one http C2 profile")

            c2_profiles = []
            for c2 in self.c2info:
                params = dict(c2.get_parameters_dict())
                name = c2.get_c2profile()["name"]
                if name != "http":
                    raise ValueError(f"unsupported C2 profile '{name}'")
                aes = params.pop("AESPSK", None)
                params["aes_psk"] = _extract_aes_psk(aes)
                if params.get("encrypted_exchange_check"):
                    raise ValueError("http encrypted_exchange_check is not implemented")
                c2_profiles.append({name: params})

            config = {"payload_uuid": self.uuid, "c2_profiles": c2_profiles}
            stdout += "[+] Packed http C2 profile into config.json.\n"
            stdout += "[+] Step 1 (Initialize Configuration) completed successfully.\n"
            await _update_build_step(
                self.uuid, "Initialize Configuration", stdout, success=True
            )

        except Exception as e:
            stderr = f"[-] Step 1 (Initialize Configuration) Failed: {e}\n\n{traceback.format_exc()}"
            resp.build_stdout = stdout
            resp.build_stderr = stderr
            resp.build_message = stderr
            await _update_build_step(
                self.uuid,
                "Initialize Configuration",
                stdout,
                stderr=stderr,
                success=False,
            )
            return resp

        try:
            stdout += "\n[*] Executing Step 2: Pre-Build Verification...\n"
            _resolve_tool("cargo")
            _resolve_tool("cargo-zigbuild")
            stdout += "[+] cargo and cargo-zigbuild are available.\n"
            stdout += "[+] Runtime sandbox, stack spoofing, DLL blocking, shellcode, DLL, service output, and multi-C2 packaging are disabled until implemented.\n"
            stdout += "[+] Step 2 (Pre-Build Verification) completed successfully.\n"
            await _update_build_step(
                self.uuid, "Pre-Build Verification", stdout, success=True
            )

        except Exception as e:
            stderr = f"[-] Step 2 Unexpected Exception: {e}\n\n{traceback.format_exc()}"
            resp.build_stdout = stdout
            resp.build_stderr = stderr
            resp.build_message = stderr
            await _update_build_step(
                self.uuid,
                "Pre-Build Verification",
                stdout,
                stderr=stderr,
                success=False,
            )
            return resp

        try:
            stdout += "\n[*] Executing Step 3: Executing Production Compilation...\n"
            tmp = tempfile.TemporaryDirectory(prefix="nanaz-windows-build-")
            build_root = Path(tmp.name) / "agent_code"
            shutil.copytree(
                self.agent_code_path,
                build_root,
                ignore=shutil.ignore_patterns(
                    "target",
                    "config.json",
                    "__pycache__",
                    "*.pyc",
                ),
            )
            (build_root / "config.json").write_text(
                json.dumps(config, indent=4), encoding="utf-8"
            )

            profile_flag = "" if debug else "--release"
            command = f"cargo zigbuild {profile_flag} --target {target}".strip()
            returncode, cmd_stdout, cmd_stderr = await execute_command(
                command=command,
                cwd=build_root,
                env=_build_env(),
            )

            if cmd_stdout.strip():
                stdout += f"[Build Stdout]\n{cmd_stdout}\n"
            if cmd_stderr.strip():
                stdout += f"[Build Stderr]\n{cmd_stderr}\n"

            if returncode != 0:
                stdout += f"\n[-] CRITICAL: Compilation failed with exit code {returncode}.\n"
                resp.build_stdout = stdout
                resp.build_stderr = cmd_stderr if cmd_stderr.strip() else cmd_stdout
                resp.build_message = resp.build_stderr
                await _update_build_step(
                    self.uuid,
                    "Execute Compilation",
                    stdout,
                    stderr=resp.build_stderr,
                    success=False,
                )
                tmp.cleanup()
                return resp

            stdout += "[+] Production compilation succeeded. Artifacts successfully generated.\n"
            stdout += "[+] Step 3 (Execute Compilation) completed successfully.\n"
            await _update_build_step(
                self.uuid, "Execute Compilation", stdout, success=True
            )

        except Exception as e:
            stderr = f"[-] Step 3 Unexpected Exception: {e}\n\n{traceback.format_exc()}"
            resp.build_stdout = stdout
            resp.build_stderr = stderr
            resp.build_message = stderr
            await _update_build_step(
                self.uuid,
                "Execute Compilation",
                stdout,
                stderr=stderr,
                success=False,
            )
            return resp

        try:
            stdout += "\n[*] Executing Step 4: Package & Deliver Artifact...\n"
            profile = "debug" if debug else "release"
            binary = build_root / "target" / target / profile / "nanaz.exe"
            if not binary.exists():
                raise FileNotFoundError(f"binary not found: {binary}")
            resp.payload = binary.read_bytes()
            filename = pathlib.Path(self.filename).stem
            resp.updated_filename = f"{filename}.exe"
            resp.status = cast(Any, BuildStatus.Success)
            if dropped_commands:
                resp.build_message = (
                    "Removed commands unsupported on Windows: "
                    + ", ".join(dropped_commands)
                )
            stdout += f"[+] Packaged {binary.name} ({len(resp.payload)} bytes).\n"
            stdout += "[+] Step 4 (Package & Deliver Artifact) completed successfully.\n"
            await _update_build_step(
                self.uuid, "Package & Deliver Artifact", stdout, success=True
            )

        except Exception as e:
            stderr = f"[-] Step 4 Unexpected Exception: {e}\n\n{traceback.format_exc()}"
            resp.status = cast(Any, BuildStatus.Error)
            resp.build_stdout = stdout
            resp.build_stderr = stderr
            resp.build_message = stderr
            await _update_build_step(
                self.uuid,
                "Package & Deliver Artifact",
                stdout,
                stderr=stderr,
                success=False,
            )
            return resp

        finally:
            if build_root is not None:
                temp_dir = build_root.parent
                shutil.rmtree(temp_dir, ignore_errors=True)

        stdout += "\n[*] Executing Step 5: Workspace Cleanup...\n"
        stdout += "[+] Temporary build workspace removed.\n"
        await _update_build_step(self.uuid, "Workspace Cleanup", stdout, success=True)
        resp.build_stdout = stdout
        return resp


async def execute_command(
    command: str,
    cwd: Path | str,
    timeout: int = 600,
    env: dict[str, str] | None = None,
) -> tuple[int, str, str]:
    cwd_str = str(Path(cwd).resolve())

    process = await asyncio.create_subprocess_shell(
        command,
        cwd=cwd_str,
        env=env or os.environ.copy(),
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )

    try:
        stdout, stderr = await asyncio.wait_for(process.communicate(), timeout=timeout)
    except asyncio.TimeoutError:
        try:
            process.kill()
        except ProcessLookupError:
            pass
        raise RuntimeError(f"Command timed out after {timeout} seconds.")

    code = process.returncode
    code = -1 if code is None else code
    out_text = stdout.decode("utf-8", errors="replace")
    err_text = stderr.decode("utf-8", errors="replace")

    return code, out_text, err_text
