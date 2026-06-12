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
