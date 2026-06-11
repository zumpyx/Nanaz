# nanaz — Cross-platform Rust agent for Mythic

A Mythic payload type written in Rust. Single source tree, cross-compiled to
Windows + Linux via `cargo-zigbuild` and the `mythic-c2` C2 framework. Targets
operations that want a small, hard-to-analyse binary with a focused, audited
command surface.

| | |
|---|---|
| Language | Rust (edition 2024) |
| C2 profile | `http` (Mythic 3.x `http` C2 profile) |
| Platforms | Windows `x86_64-pc-windows-gnu`, Linux `x86_64-unknown-linux-musl` |
| Memory footprint | O(chunk) during download, no pre-alloc |
| Source size | ~3 000 LoC Rust + ~1 000 LoC Python |

---

## Install in Mythic

```bash
sudo ./mythic-cli install github https://github.com/zumpyx/Nanaz
sudo ./mythic-cli start nanaz        # only the payload type container
sudo ./mythic-cli start              # or restart everything
```

`mythic-cli install github` clones the repo into Mythic's
`Payload_Types/nanaz/` and the builder runs inside the container.

---

## Build

The builder is `Payload_Type/nanaz/nanaz/mythic/agent_functions/builder.py`.
It writes a `config.json` (C2 profile + PSK + UUID) and shells out to
`cargo zigbuild --target <triple>`.

```bash
# Inside the nanaz container:
cat > /tmp/build.sh <<'EOF'
cd /Mythic/nanaz/agent_code
cargo zigbuild --target x86_64-pc-windows-gnu --release
cargo zigbuild --target x86_64-unknown-linux-musl --release
EOF
bash /tmp/build.sh
```

Output:

- `nanaz/agent_code/target/x86_64-pc-windows-gnu/release/nanaz.exe`
- `nanaz/agent_code/target/x86_64-unknown-linux-musl/release/nanaz`

Both are stripped (`strip = "symbols"`) and panic-on-unwind
(`panic = "abort"`). On Windows, release builds hide the console
(`windows_subsystem = "windows"`).

---

## Run modes

```
# Foreground (debug) — eprintln! output, sleeps on SIGHUP
cargo run --release

# Daemonised (Linux release) — fork to background, detach, dup2 /dev/null
cargo run --release        # automatically daemonises
```

On Linux release, the agent:

1. Forks and exits the parent.
2. `setsid` to detach from the controlling terminal.
3. Closes fds 0/1/2 and reopens them against `/dev/null`.
4. Preserves the launch cwd so callback-relative tasking stays stable.
5. Ignores `SIGHUP`.

The Mythic container picks up the built binary; operators see the file
in the Mythic UI's payload-type page.

---

## C2 profile

This agent speaks Mythic's `http` C2 profile. The container's
`config.json` (per-payload, generated at build time) carries:

```json
{
  "payload_uuid": "...",
  "c2_profiles": [
    {
      "http": {
        "aes_psk": "...",
        "callback_host": "https://...",
        "callback_interval": 10,
        "callback_jitter": 23,
        "callback_port": 80,
        "encrypted_exchange_check": false,
        "get_uri": "index",
        "post_uri": "data",
        "query_path_name": "q",
        "headers": { "User-Agent": "..." },
        "killdate": "2099-12-31",
        "external_ip_check": false
      }
    }
  ]
}
```

### Configuration knobs

| Field | Default | Effect |
|---|---|---|
| `external_ip_check` | `false` | When `true`, the agent queries `https://api.ipify.org` at check-in. Off by default — the egress is a strong blue-team indicator. |
| `callback_interval` | required | Seconds between polls. `0` is allowed (busy-loop, useful for testing). |
| `callback_jitter` | `0` | Percent of `interval` added as random extra sleep. |
| `killdate` | empty | ISO date (`YYYY-MM-DD`). After this, the agent exits before the next round. |

---

## Commands

| Command | Args | Notes |
|---|---|---|
| `cat` | `path` | Read file. Cross-platform encoding detection (UTF-8 → Windows ANSI code page fallback). |
| `cd` / `pwd` | `path` / — | Change or print the agent process cwd; mirrors cwd into Mythic callback state. |
| `cp` | `src dst` | Cross-platform. Auto-creates dst parent. |
| `download` | `path` | Multi-chunk, streaming. Default 512 KiB chunks (configurable via `chunk_size`). Refuses files larger than 4 GiB. |
| `env` | `[key]` | List env vars, optionally filtered by substring. |
| `execute_assembly` / `executeAssembly` | `Assembly.exe [args] [timeout]` | Windows-only. Execute a .NET assembly in an isolated worker via `rustclr`; supports uploaded or registered assemblies. |
| `exit` | `process` / `thread` | Stop the beacon. `process` flushes pending then exits; `thread` is a legacy alias. |
| `ls` | `[path] [-r]` | File browser. `~` expansion, recursive mode, dirs first sort. |
| `mkdir` | `path` | `mkdir -p` semantics. |
| `mv` | `src dst` | Renames; falls back to copy+delete on `EXDEV` (cross-filesystem). |
| `netstat` | — | TCP/UDP connection table. Linux: `/proc/net/tcp{,6}` + `udp{,6}`. macOS: `netstat -an -W -p tcp,udp`. Windows: `netstat -ano`. |
| `ps` | — | Process listing. Linux: `/proc` walk. macOS: `ps`. Windows: `wmic` (with `tasklist` fallback). |
| `powerpick` / `PowerPick` | `command [timeout]` | Windows-only. Execute PowerShell in an isolated worker via `rustclr` without spawning `powershell.exe`. |
| `resolve` | `hostname` | DNS resolve. `std::net::ToSocketAddrs`. |
| `rm` | `path [-r]` | File browser. `recursive=true` for directories; `recursive` requires `confirm_destructive=true` to prevent typos. |
| `shell` | `command [shell] [timeout]` | Run via `cmd` / `powershell` / `bash` / `sh`. Default timeout 60s. |
| `sleep` | `interval [jitter]` | Change polling cadence at runtime. |
| `sysinfo` | — | OS, kernel, CPU, memory, uptime. |
| `upload` | `path` + file | Base64 in `file_bytes`, or Mythic file chunk pull via `file_id`. Max 256 MiB. |
| `wget` | `url [path]` | HTTPS GET, write to disk. |
| `whoami` | — | `user`, `uid`/`gid` (Unix), `home`, `hostname`. |

### UI integration

- File browser: `ls`, `download`, `upload`, `rm`
- Process browser: `ps`
- MITRE ATT&CK mapping on every command (`attackmapping = [...]`)

---

## Repo layout

```
.
├── Payload_Type/nanaz/             # Mythic container
│   ├── main.py                     # mythic_container entry point
│   ├── Dockerfile                  # python_base + Rust + zig
│   ├── rabbitmq_config.json        # (gitignored, see .gitignore)
│   └── nanaz/
│       ├── agent_code/             # Rust source
│       │   ├── Cargo.toml
│       │   ├── config.json         # (gitignored — built per payload)
│       │   ├── config.example.json
│       │   └── src/
│       │       ├── main.rs         # entry, daemonisation
│       │       ├── agent.rs        # beacon loop, killdate, jitter
│       │       ├── config.rs       # embedded JSON loader
│       │       ├── dispatch.rs     # command -> handler routing
│       │       ├── c2/             # http C2 transport
│       │       ├── sys/            # metadata, network, encoding
│       │       └── commands/       # 18 command handlers
│       └── mythic/agent_functions/ # 18 Python command defs
│           ├── _base.py            # FileBrowserArguments + helpers
│           ├── builder.py
│           └── <command>.py
├── agent_icons/nanaz.svg
├── config.json                     # Mythic install config
└── README.md
```

---

## Development

```bash
cd Payload_Type/nanaz/nanaz/agent_code
cargo check                # quick syntax + types
cargo test                 # 10 unit tests (no Mythic required)
cargo check --target x86_64-pc-windows-gnu   # cross-platform sanity
cargo build --release      # full optimised build (Linux)
```

Tests cover `cat`, `download`, `ls`, `upload`, `ps`, `encoding` and the
embedded config loader. End-to-end Mythic communication requires a
running Mythic instance — see `mythic-docs/`.

### Toolchain

The container pins:

- `RUST_VERSION=1.85.0`
- `ZIG_VERSION=0.16.0`
- `CARGO_ZIGBUILD_VERSION=0.19.10`

to avoid surprises when lockfile v4 parsing or zigbuild upstream
breaking changes hit.

---

## Security notes

- **AES PSK** is generated per-payload by Mythic and embedded at build
  time. Treat `agent_code/config.json` as a secret — it is gitignored.
- **TLS** uses the HTTP client's normal certificate validation. Use a
  trusted certificate for HTTPS C2, or use HTTP in isolated test labs.
- **Upload** writes through a temporary file and atomically replaces the
  destination when possible. Max 256 MiB per upload.
- **Download** reads in chunks so memory is bounded, and refuses files
  larger than 4 GiB.
- The agent does **not** support `encrypted_exchange_check` (the
  Noise_KK EKE handshake). Payload builds and runtime configs that enable
  it fail closed instead of silently downgrading.

---

## License

See [LICENSE](LICENSE).
