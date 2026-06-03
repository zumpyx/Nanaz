//! Build script: guarantee that `config.json` exists at the workspace
//! root before the binary's `include_str!` macro runs.
//!
//! Layout (this script runs in the same directory as `Cargo.toml`):
//!   agent_code/
//!     Cargo.toml
//!     build.rs
//!     config.json       <- created here if missing, rewritten by Mythic
//!                          builder at payload-build time
//!
//! Why this exists:
//!   - `config.json` carries per-payload secrets (AES PSK, callback UUID)
//!     and is gitignored — a fresh clone has no file.
//!   - `include_str!("../config.json")` is a compile-time macro; without
//!     a file, cargo errors out with a path-not-found message that's hard
//!     to map back to the actual cause.
//!   - This script materialises a placeholder `config.json` on the fly
//!     so `cargo check` / `cargo test` work in any working copy, then
//!     relies on the agent's own `Config::load_json` to fail loud at
//!     startup if the placeholder isn't a real payload. That preserves
//!     the H3 fail-loud semantics — the operator still gets a clean
//!     `exit 2` rather than a silent no-op agent.
//!
//! The placeholder deliberately uses values that `Config::load_json`
//! will reject: the UUID parses (it is a v4 UUID), but the
//! `aes_psk = "REPLACE_WITH_32_BYTE_BASE64_KEY"` is not valid base64
//! and is not 32 bytes after decoding, so PSK validation will fail
//! at startup. The Mythic builder overwrites this file before
//! compilation, so a production build embeds real values.

use std::path::Path;

const PLACEHOLDER: &str = r#"{
    "payload_uuid": "00000000-0000-4000-8000-000000000000",
    "c2_profiles": [
        {
            "http": {
                "aes_psk": "REPLACE_WITH_32_BYTE_BASE64_KEY",
                "callback_host": "https://your-c2.example.com",
                "callback_interval": 10,
                "callback_jitter": 23,
                "callback_port": 443,
                "encrypted_exchange_check": false,
                "get_uri": "index",
                "headers": {
                    "User-Agent": "nanaz/0.1.0 (placeholder; replace via Mythic builder)"
                },
                "killdate": "2099-12-31",
                "post_uri": "data",
                "proxy_host": "",
                "proxy_pass": "",
                "proxy_port": "",
                "proxy_user": "",
                "query_path_name": "q"
            }
        }
    ]
}
"#;

fn main() {
    let cfg = Path::new("config.json");

    // Re-run only if config.json itself is created or changed.
    // (If it doesn't exist, the next invocation will rerun and create
    // it; if it does, we rerun whenever its content changes.)
    if cfg.exists() {
        println!("cargo:rerun-if-changed=config.json");
    } else {
        // No file: materialise the placeholder so include_str! has
        // something to embed. We deliberately do NOT set
        // rerun-if-changed for the freshly created file: by the time
        // cargo invokes include_str! below, the file already exists
        // and a re-run with the same content would be a no-op.
        std::fs::write(cfg, PLACEHOLDER)
            .unwrap_or_else(|e| panic!("build.rs: failed to write placeholder config.json: {e}"));
        println!(
            "cargo:warning=wrote placeholder config.json — replace via Mythic builder before deploying"
        );
    }
}
