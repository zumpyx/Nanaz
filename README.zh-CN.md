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
