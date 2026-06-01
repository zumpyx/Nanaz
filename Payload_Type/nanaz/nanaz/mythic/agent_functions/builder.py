import asyncio
import json
import pathlib
import traceback

from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
from mythic_container.PayloadBuilder import *

TARGETS = {
    "Windows": "x86_64-pc-windows-gnu",
    "Linux": "x86_64-unknown-linux-musl",
}


class Nanaz(PayloadType):
    name = "nanaz"
    file_extension = "exe"
    author = "@zumpyx"
    mythic_encrypts = True
    supported_os = [SupportedOS.Windows, SupportedOS.Linux]
    semver = "0.1.0"
    wrapper = False
    wrapped_payloads = []
    c2_profiles = ["http", "httpx"]
    note = f"Cross-platform Rust agent. Version: {semver}."
    supports_dynamic_loading = True
    supports_multiple_c2_instances_in_build = True
    supports_multiple_c2_in_build = True

    build_parameters = [
        BuildParameter(
            name="debug",
            parameter_type=BuildParameterType.Boolean,
            default_value=False,
            description="Build with debug symbols.",
        ),
    ]

    agent_code_path = pathlib.Path(".") / "nanaz" / "agent_code"

    async def build(self) -> BuildResponse:
        resp = BuildResponse(status=BuildStatus.Error)

        try:
            debug = self.get_parameter("debug")
            target_os = "Windows" if "windows" in str(getattr(self, "selected_os", "")).lower() else "Linux"

            # --- config.json ---
            c2_profiles = []
            for c2 in self.c2info:
                params = dict(c2.get_parameters_dict())
                name = c2.get_c2profile()["name"]
                if name == "http":
                    aes = params.pop("AESPSK", None)
                    params["aes_psk"] = aes.get("enc_key", "") if isinstance(aes, dict) else (aes or "")
                c2_profiles.append({name: params})

            config = {"payload_uuid": self.uuid, "c2_profiles": c2_profiles}
            self.agent_code_path.mkdir(parents=True, exist_ok=True)
            config_path = self.agent_code_path / "config.json"
            config_path.write_text(json.dumps(config, indent=4))

            # --- compile ---
            triple = TARGETS.get(target_os)
            cargo_args = ["zigbuild", "--target", triple]
            if not debug:
                cargo_args.insert(1, "-r")

            proc = await asyncio.create_subprocess_exec(
                "/usr/local/cargo/bin/cargo",
                *cargo_args,
                cwd=str(self.agent_code_path),
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT,
            )

            async for line in proc.stdout:
                print(line.decode("utf-8", errors="ignore").rstrip(), flush=True)
            await proc.wait()

            if proc.returncode != 0:
                raise Exception(f"cargo zigbuild failed (exit {proc.returncode})")

            # --- collect artifact ---
            profile = "debug" if debug else "release"
            binary = self.agent_code_path / "target" / triple / profile / "nanaz"
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
