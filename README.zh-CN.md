<<<<<<< HEAD
# nanaz

nanaz 是一个跨平台的 Mythic payload type / agent 扩展，使用 Rust 编写。
它从同一套代码构建 Windows 与 Linux agent，并集成 Mythic 的 `http` C2
profile。

本项目用于学习和研究用途，没有实现 EDR 规避、隐蔽增强或绕过技术。

## 已测试平台

- Windows amd64：`x86_64-pc-windows-gnu`
- Linux amd64：`x86_64-unknown-linux-musl`

其他架构或操作系统可能可以通过额外适配完成构建，但目前未验证。

## 支持功能

- Mythic payload type builder
- 按 payload 选择命令
- Mythic `http` C2 profile
- AES-PSK Mythic 消息加密
- Callback 元数据与 sleep 间隔更新
- 文件浏览器集成
- 进程浏览器集成
- Mythic 文件上传和下载
- SOCKS 代理
- 反向端口转发（`rpfwd`）
- Linux PTY interactive task
- Windows .NET assembly 隔离 worker 执行
- Windows PowerShell 执行：`powershell` 与 `powerpick`
- 跨平台 shell / 进程执行

## 命令

跨平台命令：

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

Windows 专属命令：

- `cmd`
- `execute_assembly`
- `powershell`
- `powerpick`

Linux 专属命令：

- `bash`
- `pty`
- `sh`

## 安装

```bash
sudo ./mythic-cli install github https://github.com/zumpyx/Nanaz
sudo ./mythic-cli start nanaz
```

## 构建说明

Mythic builder 会写入 payload 配置，并使用 `cargo zigbuild` 交叉编译
agent。

Release 目标：

- Windows：`x86_64-pc-windows-gnu`
- Linux：`x86_64-unknown-linux-musl`

当前每个 payload build 支持一个 `http` C2 profile。

## 测试状态

已在 amd64 Windows 与 amd64 Linux 上完成端到端测试：

- callback 注册与 tasking
- 文件浏览器与进程浏览器
- upload、download、`wget`
- SOCKS 与 `rpfwd`
- Windows `powerpick` 与 `execute_assembly`
- sleep 与 exit 行为

## 开发

```bash
cd Payload_Type/nanaz/nanaz/agent_code
cargo fmt --check
cargo test
```

## License

见 [LICENSE](LICENSE)。
=======
# Nanaz

[English](README.md)

Nanaz 是一个跨平台 Mythic payload type / agent 扩展。Agent 使用 Rust 编写，
Mythic container 和命令定义使用 Python 编写。

本项目用于学习和实验环境。当前没有实现 EDR 规避、隐蔽加载、sleep mask、
syscall 规避等对抗能力。目前只测试了 amd64 Windows 和 amd64 Linux 平台。

## Payload Type

- `nanaz`：原始跨平台 payload type。
- `nanaz-windows`：面向 Windows 的 payload type。
- `nanaz-linux`：面向 Linux 的 payload type。

## 已测试平台

- Windows：`x86_64-pc-windows-gnu`，Portable Executable。
- Linux：`x86_64-unknown-linux-musl`，静态 ELF。

## C2

- Mythic `http` C2 profile。
- 每个 payload build 当前只支持一个 C2 profile。
- Mythic 生成的 AES-PSK 会在构建时嵌入。
- 当前未实现 `encrypted_exchange_check`。

## 当前支持功能

- Callback 元数据采集。
- 运行时修改 sleep 和 jitter。
- `exit` 任务会先 flush 待发送响应。
- 文件浏览器集成：`drives`、`ls`、`tree`、`download`、`upload`、`rm`。
- 进程浏览器集成：`ps`、`kill`。
- 文件操作：`cat`、`cd`、`pwd`、`cp`、`mv`、`mkdir`、`rm`。
- 环境与主机信息：`env`、`sysinfo`、`whoami`、`netstat`、`resolve`。
- 进程执行：
  - Windows：`cmd`、`powershell`、`execute`、`powerpick`、`execute_assembly`。
  - Linux：`sh`、`bash`、`execute`、`pty`。
- 文件传输：
  - Mythic 分块 `upload` / `download`。
  - 通过 `wget` 从 URL 下载文件。
- SOCKS5 代理。
- 反向端口转发。
- PTY 风格 interactive task。

## 构建

在对应 payload type 目录运行：

```bash
cd Payload_Type/nanaz-windows
python3 main.py
```

或：

```bash
cd Payload_Type/nanaz-linux
python3 main.py
```

通常由 Mythic 在 payload type container 同步后发起构建。本地可用以下命令检查
agent 构建：

```bash
cd Payload_Type/nanaz-windows/nanaz_windows/agent_code
cargo zigbuild --target x86_64-pc-windows-gnu --release

cd Payload_Type/nanaz-linux/nanaz_linux/agent_code
cargo zigbuild --target x86_64-unknown-linux-musl --release
```

## 开发检查

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

跨 target 跑 clippy 时，需要提供目标平台 C 编译器，或使用 `zig cc` wrapper。

## 说明

- `supports_dynamic_loading=True` 用于 Mythic 构建时命令选择。当前没有实现运行时
  命令动态加载。
- DLL、service executable、shellcode 输出、多 C2 打包、sandbox 检查、stack
  spoofing、syscall 选项和 sleep mask 尚未实现，因此不会作为可用选项暴露。
- `agent_code/config.json` 是每个 payload 构建时生成的配置，包含构建期 secret，
  不应提交。
>>>>>>> dev
