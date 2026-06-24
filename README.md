<<<<<<< HEAD
# nanaz

[简体中文](README.zh-CN.md)

nanaz is a cross-platform Mythic payload type / agent extension written in
Rust. It builds Windows and Linux agents from one codebase and integrates with
Mythic's `http` C2 profile.

This project is intended for learning and research. It does not implement EDR
evasion, stealth hardening, or bypass techniques.

## Tested Platforms

- Windows amd64: `x86_64-pc-windows-gnu`
- Linux amd64: `x86_64-unknown-linux-musl`

Other architectures or operating systems may build with additional work, but
they have not been validated.

## Supported Features

- Mythic payload type builder
- Per-payload command selection
- Mythic `http` C2 profile
- AES-PSK Mythic message encryption
- Callback metadata and sleep interval updates
- File browser integration
- Process browser integration
- Upload and download with Mythic file transfer
- SOCKS proxy
- Reverse port forward (`rpfwd`)
- Interactive task support for PTY on Linux
- Windows .NET assembly execution via isolated worker
- Windows PowerShell execution via `powershell` and `powerpick`
- Cross-platform shell/process execution

## Commands

Cross-platform commands:

- `cat`
- `cd`
- `cp`
- `download`
- `drives`
- `env`
- `execute`
- `exit`
- `kill`
- `ls`
- `mkdir`
- `mv`
- `netstat`
- `ps`
- `pwd`
- `resolve`
- `rm`
- `rpfwd`
- `sleep`
- `socks`
- `sysinfo`
- `tree`
- `upload`
- `wget`
- `whoami`

Windows-only commands:

- `cmd`
- `execute_assembly`
- `powershell`
- `powerpick`

Linux-only commands:

- `bash`
- `pty`
- `sh`

## Install

```bash
sudo ./mythic-cli install github https://github.com/zumpyx/Nanaz
sudo ./mythic-cli start nanaz
```

## Build Notes

The Mythic builder writes the payload configuration and cross-compiles the agent
with `cargo zigbuild`.

Release targets:

- Windows: `x86_64-pc-windows-gnu`
- Linux: `x86_64-unknown-linux-musl`

The agent currently supports one `http` C2 profile per payload build.

## Tested Status

End-to-end testing has been performed on amd64 Windows and amd64 Linux with:

- callback registration and tasking
- file browser and process browser
- upload, download, and `wget`
- SOCKS and `rpfwd`
- Windows `powerpick` and `execute_assembly`
- sleep and exit behavior

## Development

```bash
cd Payload_Type/nanaz/nanaz/agent_code
cargo fmt --check
cargo test
```

## License

See [LICENSE](LICENSE).
=======
# Nanaz

[中文版](README.zh-CN.md)

Nanaz is a cross-platform Mythic payload type / agent extension written in Rust
with Mythic container definitions in Python.

This project is intended for learning and lab use. It does not implement EDR
evasion, stealth loading, sleep masking, syscall evasion, or similar tradecraft.
Only amd64 Windows and amd64 Linux builds have been tested.

## Payload Types

- `nanaz`: original cross-platform payload type.
- `nanaz-windows`: Windows-focused payload type.
- `nanaz-linux`: Linux-focused payload type.

## Tested Platforms

- Windows: `x86_64-pc-windows-gnu`, Portable Executable.
- Linux: `x86_64-unknown-linux-musl`, static ELF.

## C2

- Mythic `http` C2 profile.
- Single C2 profile per payload build.
- AES-PSK from Mythic is embedded at build time.
- `encrypted_exchange_check` is currently not implemented.

## Supported Features

- Callback metadata collection.
- Runtime sleep and jitter update.
- Exit task with response flush.
- File browser integration: `drives`, `ls`, `tree`, `download`, `upload`, `rm`.
- Process browser integration: `ps`, `kill`.
- File operations: `cat`, `cd`, `pwd`, `cp`, `mv`, `mkdir`, `rm`.
- Environment and host info: `env`, `sysinfo`, `whoami`, `netstat`, `resolve`.
- Process execution:
  - Windows: `cmd`, `powershell`, `execute`, `powerpick`, `execute_assembly`.
  - Linux: `sh`, `bash`, `execute`, `pty`.
- File transfer:
  - Mythic chunked `upload` / `download`.
  - URL download via `wget`.
- SOCKS5 proxy.
- Reverse port forward.
- Interactive task support for PTY-style sessions.

## Build

Run from the relevant payload type directory:

```bash
cd Payload_Type/nanaz-windows
python3 main.py
```

or:

```bash
cd Payload_Type/nanaz-linux
python3 main.py
```

Builds are normally started from Mythic after the payload type container syncs.
Local agent builds can be checked with:

```bash
cd Payload_Type/nanaz-windows/nanaz_windows/agent_code
cargo zigbuild --target x86_64-pc-windows-gnu --release

cd Payload_Type/nanaz-linux/nanaz_linux/agent_code
cargo zigbuild --target x86_64-unknown-linux-musl --release
```

## Development Checks

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

For cross-target clippy, provide a target C compiler or a `zig cc` wrapper.

## Notes

- `supports_dynamic_loading=True` is used so Mythic build-time command
  selection works. Runtime command loading is not implemented.
- DLL, service executable, shellcode output, multi-C2 packaging, sandbox checks,
  stack spoofing, syscall options, and sleep masking are not exposed until they
  are implemented.
- `agent_code/config.json` is generated per payload and contains build-time
  secrets. It must not be committed.
>>>>>>> dev
