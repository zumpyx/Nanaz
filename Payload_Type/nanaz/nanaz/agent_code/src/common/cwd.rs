//! Process cwd coordination.
//!
//! Rust exposes cwd as process-global state. The agent executes tasks on
//! multiple worker threads, so commands that read or mutate cwd must share a
//! single lock or relative paths become scheduler-dependent.

use std::sync::{Mutex, OnceLock};

static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn with_cwd_lock<R>(f: impl FnOnce() -> R) -> R {
    let _guard = CWD_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f()
}
