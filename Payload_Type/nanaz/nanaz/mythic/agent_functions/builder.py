import asyncio
import json
import os
import pathlib
import shutil
import tempfile
import traceback

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
from mythic_container.PayloadBuilder import *

TARGETS = {
    "Windows": "x86_64-pc-windows-gnu",
    "Linux": "x86_64-unknown-linux-musl",
}

# Resolve paths from this file's location so the builder works regardless of
# the container's CWD. Layout: nanaz/{agent_code, mythic}. builder.py lives at
# nanaz/mythic/agent_functions/builder.py, so three parents reaches nanaz/.
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
    """Resolve build tools inside Mythic containers without hardcoding one image."""
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


def _build_env() -> dict:
    env = os.environ.copy()
    path_entries = [str(path) for path in TOOL_DIRS]
    if env.get("PATH"):
        path_entries.append(env["PATH"])
    env["PATH"] = os.pathsep.join(dict.fromkeys(path_entries))
    return env


def _read_cargo_semver() -> str:
    """Read the agent version from the `[package]` section of `Cargo.toml`.

    Falls back to a hardcoded string if the file cannot be parsed so a
    broken sync never bricks the payload-type container — the operator
    still sees a version, just not necessarily the right one.

    Naive string matching is intentional: a full TOML parser would be
    overkill for a single scalar, and we explicitly anchor on the
    `[package]` section header to avoid hitting a dep's `version = "..."`.
    """
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


COMMAND_HELP = [
    ("bash", "bash [command]", "Run a Bash command."),
    ("cat", "cat [path]", "Read and display file contents."),
    ("cd", "cd [path]", "Change the current working directory."),
    ("cmd", "cmd [command]", "Run a Windows cmd.exe command."),
    ("cp", "cp [src] [dst]", "Copy a file."),
    ("download", "download [path]", "Download a file from the target."),
    ("drives", "drives", "List available filesystem roots / drives."),
    ("env", "env [filter_key]", "List environment variables."),
    ("execute", "execute [path] [arguments]", "Execute a process."),
    (
        "execute_assembly",
        "execute_assembly [Assembly.exe] [args]",
        "Execute a .NET assembly.",
    ),
    ("exit", "exit [process]", "Exit the agent or callback."),
    ("help", "help [command]", "Show command help."),
    ("kill", "kill <pid> [-9]", "Kill a process."),
    ("ls", "ls [path]", "List files and directories."),
    ("mkdir", "mkdir [path]", "Create a directory."),
    ("mv", "mv [src] [dst]", "Move or rename a file."),
    ("netstat", "netstat", "List network connections."),
    ("powerpick", "powerpick [command]", "Run PowerShell through CLR hosting."),
    ("powershell", "powershell [command]", "Run a PowerShell command."),
    ("ps", "ps", "List processes for Mythic's process browser."),
    ("pwd", "pwd", "Print the current working directory."),
    ("resolve", "resolve [hostname]", "Resolve a hostname."),
    (
        "rm",
        "rm [path] [-r] [--confirm-destructive]",
        "Remove a file or directory.",
    ),
    ("sh", "sh [command]", "Run a POSIX shell command."),
    ("sleep", "sleep [seconds] [jitter]", "Set callback sleep and jitter."),
    ("sysinfo", "sysinfo", "Gather system information."),
    ("tree", "tree [path]", "Recursively list a directory tree."),
    ("upload", "upload [destination_path]", "Upload a file to the target."),
    ("wget", "wget [url] [destination_path]", "Download a URL to disk."),
    ("whoami", "whoami", "Print the current user."),
]


def _format_help(command_names: list[str] | None = None) -> str:
    wanted = {name.strip().lower() for name in command_names or [] if name.strip()}
    rows = [
        row
        for row in COMMAND_HELP
        if not wanted or row[0] in wanted
    ]
    if not rows:
        missing = ", ".join(sorted(wanted))
        return f"unknown command(s): {missing}"

    if len(rows) == 1 and wanted:
        command, usage, description = rows[0]
        return f"{command}\nUsage: {usage}\n{description}"

    lines = ["Available commands:"]
    for command, usage, description in rows:
        lines.append(f"  {command:<16} {usage:<42} {description}")
    return "\n".join(lines)


class Nanaz(PayloadType):
    name = "nanaz"
    file_extension = "exe"
    author = "@zumpyx"
    mythic_encrypts = True
    supported_os = [SupportedOS.Windows, SupportedOS.Linux]
    # Authoritative source of the agent version is the Rust crate
    # (`Cargo.toml`). Reading it at import time keeps the builder from
    # drifting out of sync, and keeps the displayed note consistent.
    semver = _read_cargo_semver()
    wrapper = False
    wrapped_payloads = []
    # httpx is intentionally NOT listed — only the http C2 profile is
    # implemented in src/c2/. Adding it would surface unsupported options
    # in the operator UI.
    c2_profiles = ["http"]
    note = f"Cross-platform Rust agent. Version: {semver}."
    supports_dynamic_loading = False
    supports_multiple_c2_instances_in_build = False
    supports_multiple_c2_in_build = False

    build_parameters = [
        BuildParameter(
            name="debug",
            parameter_type=BuildParameterType.Boolean,
            default_value=False,
            description="Build with debug symbols.",
        ),
    ]

    agent_path = MYTHIC_PATH
    agent_icon_path = agent_path / "agent_functions" / "nanaz.svg"
    agent_code_path = AGENT_CODE_PATH

    async def command_help_function(
        self, msg: HelpFunctionMessage
    ) -> HelpFunctionMessageResponse:
        return HelpFunctionMessageResponse(
            output=_format_help(msg.CommandNames),
            success=True,
        )

    async def build(self) -> BuildResponse:
        resp = BuildResponse(status=BuildStatus.Error)

        try:
            debug = self.get_parameter("debug")
            selected = str(getattr(self, "selected_os", "")).lower()
            if "windows" in selected:
                target_os = "Windows"
            elif "linux" in selected:
                target_os = "Linux"
            else:
                raise Exception(
                    f"unsupported selected_os '{selected}'; only Windows and Linux are supported"
                )

            if len(self.c2info) != 1:
                raise Exception(
                    "nanaz supports exactly one http C2 profile per payload build"
                )

            # --- config.json ---
            c2_profiles = []
            for c2 in self.c2info:
                params = dict(c2.get_parameters_dict())
                name = c2.get_c2profile()["name"]
                if name == "http":
                    aes = params.pop("AESPSK", None)
                    params["aes_psk"] = _extract_aes_psk(aes)
                    if params.get("encrypted_exchange_check"):
                        raise Exception(
                            "http encrypted_exchange_check is not implemented by nanaz"
                        )
                c2_profiles.append({name: params})

            config = {"payload_uuid": self.uuid, "c2_profiles": c2_profiles}

            # --- compile ---
            triple = TARGETS[target_os]
            cargo = _resolve_tool("cargo")
            _resolve_tool("cargo-zigbuild")
            cargo_args = ["zigbuild", "--target", triple]
            if not debug:
                cargo_args.insert(1, "-r")

            with tempfile.TemporaryDirectory(prefix="nanaz-build-") as tmp:
                build_root = pathlib.Path(tmp) / "agent_code"
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
                config_path = build_root / "config.json"
                config_path.write_text(json.dumps(config, indent=4), encoding="utf-8")

                proc = await asyncio.create_subprocess_exec(
                    cargo,
                    *cargo_args,
                    cwd=str(build_root),
                    env=_build_env(),
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.STDOUT,
                )

                while True:
                    line = await proc.stdout.readline()
                    if not line:
                        break
                    print(line.decode("utf-8", errors="ignore").rstrip(), flush=True)
                await proc.wait()

                if proc.returncode != 0:
                    raise Exception(f"cargo zigbuild failed (exit {proc.returncode})")

                # --- collect artifact ---
                profile = "debug" if debug else "release"
                binary = build_root / "target" / triple / profile / "nanaz"
                if target_os == "Windows":
                    binary = binary.with_suffix(".exe")

                if not binary.exists():
                    raise Exception(f"binary not found: {binary}")

                resp.payload = binary.read_bytes()

            # --- finalize ---
            name = pathlib.Path(self.filename).stem
            if target_os == "Windows":
                name = f"{name}.exe"
            resp.updated_filename = name
            resp.status = BuildStatus.Success

        except Exception as e:
            resp.build_message = f"build failed: {e}\n{traceback.format_exc()}"
            print(f"[-] {resp.build_message}", flush=True)

        return resp
