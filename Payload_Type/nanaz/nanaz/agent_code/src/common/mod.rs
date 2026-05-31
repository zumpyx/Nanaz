//! Shared utilities — keep small; prefer single-responsibility modules.
//!
//! All modules in here should be `no_std`-compatible (i.e. use `alloc` only,
//! no `std::fs`, `std::process`, etc.).

pub mod base64;
