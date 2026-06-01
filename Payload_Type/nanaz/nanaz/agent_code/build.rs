//! Build script: detect whether `config.json` exists at the workspace
//! root and emit a `cfg` flag the main binary uses to pick the right
//! `include_str!` source.
//!
//! Layout assumed (this script runs in the same directory as `Cargo.toml`):
//!   agent_code/
//!     Cargo.toml
//!     config.json              <- optional, gitignored, per-payload secret
//!     config.example.json      <- tracked, placeholder for `cargo check`
//!     build.rs
//!
//! If `config.json` is present, the binary embeds it (real PSK + UUID
//! from the Mythic builder). Otherwise it falls back to
//! `config.example.json` so `cargo check` / `cargo test` work in a
//! fresh clone without any pre-build step.
//!
//! Either way the agent's own `Config::load_json` validates the
//! embedded JSON at startup — a placeholder that slips through will
//! still cause a clean `exit 2` rather than a silent no-op.

use std::path::Path;

fn main() {
    let has_real = Path::new("config.json").exists();
    let has_example = Path::new("config.example.json").exists();

    // Always check-cfg first; then rustc-cfg sets the flag the binary
    // uses. `has_real_config` is the public name; the binary's
    // `#[cfg(has_real_config)]` and `#[cfg(not(has_real_config))]`
    // attributes pick the include source.
    println!("cargo:rustc-check-cfg=cfg(has_real_config)");
    if has_real {
        println!("cargo:rustc-cfg=has_real_config");
    }

    // Re-run only if the *existence* of either file might have changed
    // (rather than on every build), and only if at least one of them
    // exists — otherwise we have nothing to embed either way and a
    // missing file would still be a compile error.
    if has_real {
        println!("cargo:rerun-if-changed=config.json");
    }
    if has_example {
        println!("cargo:rerun-if-changed=config.example.json");
    } else {
        // The example file is the fall-back source; without it, the
        // `not(has_real_config)` branch cannot embed anything, so
        // surface that as a build error early.
        panic!(
            "config.example.json is missing from the agent_code/ workspace. \
             Restore it from git (it is tracked) or the `not(has_real_config)` \
             branch of src/main.rs will not compile."
        );
    }
}
