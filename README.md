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
