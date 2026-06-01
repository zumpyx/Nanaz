//! Shared utilities — keep small; prefer single-responsibility modules.
//!
//! Modules here are `alloc`-only (no `std::fs`, no `std::process`); some
//! of them (`pathguard`) use `cfg!` for the host target, but the
//! dependency is on the alloc-only `String` type, not on std.

pub mod base64;
pub mod pathguard;
