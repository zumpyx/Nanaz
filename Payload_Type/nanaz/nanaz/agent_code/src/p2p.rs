use std::collections::{HashMap, HashSet, VecDeque};

use mythic::{AlertMessage, DelegateMessage, EdgeMessage};
use uuid::Uuid;

const ALERT_SOURCE: &str = "nanaz-p2p";

#[derive(Default)]
struct PeerRoute {
    #[cfg(test)]
    c2_profile: String,
    mythic_uuid: Option<Uuid>,
    inbound_to_peer: VecDeque<String>,
}

pub struct P2pManager {
    peers: HashMap<Uuid, PeerRoute>,
    aliases: HashMap<Uuid, Uuid>,
    outbound_delegates: VecDeque<DelegateMessage>,
    outbound_edges: VecDeque<EdgeMessage>,
    outbound_alerts: VecDeque<AlertMessage>,
    warned_unroutable: HashSet<Uuid>,
}

impl P2pManager {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
            aliases: HashMap::new(),
            outbound_delegates: VecDeque::new(),
            outbound_edges: VecDeque::new(),
            outbound_alerts: VecDeque::new(),
            warned_unroutable: HashSet::new(),
        }
    }

    pub fn handle_inbound(&mut self, messages: Vec<DelegateMessage>) {
        for message in messages {
            self.handle_delegate(message);
        }
    }

    pub fn drain_delegates(&mut self) -> Vec<DelegateMessage> {
        self.outbound_delegates.drain(..).collect()
    }

    pub fn drain_edges(&mut self) -> Vec<EdgeMessage> {
        self.outbound_edges.drain(..).collect()
    }

    pub fn drain_alerts(&mut self) -> Vec<AlertMessage> {
        self.outbound_alerts.drain(..).collect()
    }

    pub fn requeue_delegates_front(&mut self, messages: Vec<DelegateMessage>) {
        for message in messages.into_iter().rev() {
            self.outbound_delegates.push_front(message);
        }
    }

    pub fn requeue_edges_front(&mut self, messages: Vec<EdgeMessage>) {
        for message in messages.into_iter().rev() {
            self.outbound_edges.push_front(message);
        }
    }

    pub fn wants_fast_poll(&self) -> bool {
        !self.outbound_delegates.is_empty()
            || !self.outbound_edges.is_empty()
            || !self.outbound_alerts.is_empty()
    }

    #[cfg(test)]
    fn register_peer(&mut self, local_uuid: Uuid, c2_profile: &str) {
        self.peers.insert(
            local_uuid,
            PeerRoute {
                c2_profile: c2_profile.to_string(),
                ..Default::default()
            },
        );
    }

    #[cfg(test)]
    fn queue_peer_message(&mut self, local_uuid: Uuid, message: &str) -> Result<(), String> {
        let route = self
            .peers
            .get(&local_uuid)
            .ok_or_else(|| "unknown peer route".to_string())?;
        self.outbound_delegates.push_back(DelegateMessage {
            message: message.to_string(),
            c2_profile: Some(route.c2_profile.clone()),
            uuid: route.mythic_uuid.unwrap_or(local_uuid),
            mythic_uuid: None,
        });
        Ok(())
    }

    #[cfg(test)]
    fn take_peer_messages(&mut self, local_uuid: Uuid) -> Vec<String> {
        self.peers
            .get_mut(&local_uuid)
            .map(|route| route.inbound_to_peer.drain(..).collect())
            .unwrap_or_default()
    }

    fn handle_delegate(&mut self, message: DelegateMessage) {
        let local_uuid = self.resolve_local_uuid(message.uuid);
        if let Some(route) = self.peers.get_mut(&local_uuid) {
            if let Some(mythic_uuid) = message.mythic_uuid {
                route.mythic_uuid = Some(mythic_uuid);
                self.aliases.insert(mythic_uuid, local_uuid);
                self.aliases.insert(message.uuid, local_uuid);
            }
            route.inbound_to_peer.push_back(message.message);
            return;
        }

        if self.warned_unroutable.insert(message.uuid) {
            self.outbound_alerts.push_back(AlertMessage {
                source: Some(ALERT_SOURCE.into()),
                level: Some("warning".into()),
                alert: Some(format!(
                    "received delegate message for unknown peer route {}",
                    message.uuid
                )),
                send_webhook: None,
                webhook_alert: None,
            });
        }
    }

    fn resolve_local_uuid(&self, uuid: Uuid) -> Uuid {
        self.aliases.get(&uuid).copied().unwrap_or(uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queues_peer_messages_as_delegates() {
        let mut manager = P2pManager::new();
        let peer_uuid = Uuid::new_v4();
        manager.register_peer(peer_uuid, "tcp");
        manager.queue_peer_message(peer_uuid, "checkin").unwrap();

        let delegates = manager.drain_delegates();
        assert_eq!(delegates.len(), 1);
        assert_eq!(delegates[0].uuid, peer_uuid);
        assert_eq!(delegates[0].c2_profile.as_deref(), Some("tcp"));
        assert_eq!(delegates[0].message, "checkin");
    }

    #[test]
    fn records_mythic_uuid_alias_and_routes_replies() {
        let mut manager = P2pManager::new();
        let local_uuid = Uuid::new_v4();
        let mythic_uuid = Uuid::new_v4();
        manager.register_peer(local_uuid, "tcp");

        manager.handle_inbound(vec![DelegateMessage {
            message: "server-response".into(),
            c2_profile: None,
            uuid: local_uuid,
            mythic_uuid: Some(mythic_uuid),
        }]);
        assert_eq!(
            manager.take_peer_messages(local_uuid),
            vec!["server-response".to_string()]
        );

        manager.queue_peer_message(local_uuid, "next").unwrap();
        let delegates = manager.drain_delegates();
        assert_eq!(delegates[0].uuid, mythic_uuid);
    }

    #[test]
    fn alerts_once_for_unroutable_delegate() {
        let mut manager = P2pManager::new();
        let peer_uuid = Uuid::new_v4();
        let message = DelegateMessage {
            message: "tasking".into(),
            c2_profile: None,
            uuid: peer_uuid,
            mythic_uuid: None,
        };
        manager.handle_inbound(vec![message.clone(), message]);

        let alerts = manager.drain_alerts();
        assert_eq!(alerts.len(), 1);
        assert!(
            alerts[0]
                .alert
                .as_deref()
                .unwrap_or_default()
                .contains(&peer_uuid.to_string())
        );
    }

    #[test]
    fn requeues_delegates_and_edges() {
        let mut manager = P2pManager::new();
        let uuid = Uuid::new_v4();
        manager.requeue_delegates_front(vec![DelegateMessage {
            message: "m".into(),
            c2_profile: Some("tcp".into()),
            uuid,
            mythic_uuid: None,
        }]);
        manager.requeue_edges_front(vec![EdgeMessage {
            source: "a".into(),
            destination: "b".into(),
            action: "add".into(),
            c2_profile: "tcp".into(),
            metadata: None,
        }]);

        assert!(manager.wants_fast_poll());
        assert_eq!(manager.drain_delegates().len(), 1);
        assert_eq!(manager.drain_edges().len(), 1);
    }
}
