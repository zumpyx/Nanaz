use std::collections::VecDeque;

use mythic::{AgentExtras, AlertMessage};

pub struct AuxiliaryManager {
    outbound_alerts: VecDeque<AlertMessage>,
    warned_delegates: bool,
    warned_edges: bool,
}

impl AuxiliaryManager {
    pub fn new() -> Self {
        Self {
            outbound_alerts: VecDeque::new(),
            warned_delegates: false,
            warned_edges: false,
        }
    }

    pub fn handle_inbound(&mut self, extras: &AgentExtras) {
        if !extras.delegates.is_empty() && !self.warned_delegates {
            self.warned_delegates = true;
            self.warn(
                "received delegate messages from Mythic; P2P is not implemented in this payload",
            );
        }
        if !extras.edges.is_empty() && !self.warned_edges {
            self.warned_edges = true;
            self.warn("received edge messages from Mythic; P2P is not implemented in this payload");
        }
    }

    pub fn drain_into(&mut self, shared: &mut AgentExtras) {
        shared.alerts.extend(self.outbound_alerts.drain(..));
    }

    pub fn requeue_alerts_front(&mut self, alerts: Vec<AlertMessage>) {
        for alert in alerts.into_iter().rev() {
            self.outbound_alerts.push_front(alert);
        }
    }

    pub fn wants_fast_poll(&self) -> bool {
        !self.outbound_alerts.is_empty()
    }

    fn warn(&mut self, message: &str) {
        self.outbound_alerts.push_back(AlertMessage {
            source: Some("nanaz".into()),
            level: Some("warning".into()),
            alert: Some(message.into()),
            send_webhook: None,
            webhook_alert: None,
        });
    }
}
