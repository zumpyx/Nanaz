use std::collections::BTreeMap;

pub const TCP_CHUNK_SIZE: usize = 30_000;
const HEADER_LEN: usize = 12;
const CHUNK_META_LEN: usize = 8;
const MAX_CHUNK_SIZE: usize = 256 * 1024;
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum TcpFrameError {
    ChunkTooLarge(usize),
    InvalidFrameLength(u32),
    InvalidChunkIndex { index: u32, total: u32 },
    MessageTooLarge(usize),
}

pub fn encode_message(message: &[u8]) -> Vec<u8> {
    encode_message_with_chunk_size(message, TCP_CHUNK_SIZE)
}

fn encode_message_with_chunk_size(message: &[u8], chunk_size: usize) -> Vec<u8> {
    assert!(chunk_size > 0, "chunk size must be non-zero");

    let total_chunks = (message.len() / chunk_size) + 1;
    let mut framed = Vec::with_capacity(message.len() + (total_chunks * HEADER_LEN));

    for chunk_index in 0..total_chunks {
        let start = chunk_index * chunk_size;
        let end = (start + chunk_size).min(message.len());
        let chunk = if start < message.len() {
            &message[start..end]
        } else {
            &[]
        };
        let frame_len = (chunk.len() + CHUNK_META_LEN) as u32;
        framed.extend_from_slice(&frame_len.to_be_bytes());
        framed.extend_from_slice(&(total_chunks as u32).to_be_bytes());
        framed.extend_from_slice(&(chunk_index as u32).to_be_bytes());
        framed.extend_from_slice(chunk);
    }

    framed
}

#[derive(Default)]
pub struct TcpFrameDecoder {
    buffer: Vec<u8>,
    current: Option<ChunkAssembly>,
}

impl TcpFrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>, TcpFrameError> {
        self.buffer.extend_from_slice(data);
        let mut completed = Vec::new();

        loop {
            if self.buffer.len() < HEADER_LEN {
                break;
            }

            let frame_len = u32::from_be_bytes([
                self.buffer[0],
                self.buffer[1],
                self.buffer[2],
                self.buffer[3],
            ]);
            if frame_len < CHUNK_META_LEN as u32 {
                return Err(TcpFrameError::InvalidFrameLength(frame_len));
            }

            let payload_len = frame_len as usize - CHUNK_META_LEN;
            if payload_len > MAX_CHUNK_SIZE {
                return Err(TcpFrameError::ChunkTooLarge(payload_len));
            }

            let needed = HEADER_LEN + payload_len;
            if self.buffer.len() < needed {
                break;
            }

            let total_chunks = u32::from_be_bytes([
                self.buffer[4],
                self.buffer[5],
                self.buffer[6],
                self.buffer[7],
            ]);
            let chunk_index = u32::from_be_bytes([
                self.buffer[8],
                self.buffer[9],
                self.buffer[10],
                self.buffer[11],
            ]);
            if total_chunks == 0 || chunk_index >= total_chunks {
                return Err(TcpFrameError::InvalidChunkIndex {
                    index: chunk_index,
                    total: total_chunks,
                });
            }

            let payload = self.buffer[HEADER_LEN..needed].to_vec();
            self.buffer.drain(..needed);
            if let Some(message) = self.add_chunk(total_chunks, chunk_index, payload)? {
                completed.push(message);
            }
        }

        Ok(completed)
    }

    fn add_chunk(
        &mut self,
        total_chunks: u32,
        chunk_index: u32,
        payload: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, TcpFrameError> {
        if self
            .current
            .as_ref()
            .is_none_or(|assembly| assembly.total_chunks != total_chunks)
        {
            self.current = Some(ChunkAssembly::new(total_chunks));
        }

        if let Some(assembly) = self.current.as_mut() {
            assembly.push(chunk_index, payload)?;
            if assembly.is_complete()
                && let Some(assembly) = self.current.take()
            {
                return Ok(Some(assembly.into_message()));
            }
        }
        Ok(None)
    }
}

struct ChunkAssembly {
    total_chunks: u32,
    chunks: BTreeMap<u32, Vec<u8>>,
    total_size: usize,
}

impl ChunkAssembly {
    fn new(total_chunks: u32) -> Self {
        Self {
            total_chunks,
            chunks: BTreeMap::new(),
            total_size: 0,
        }
    }

    fn push(&mut self, chunk_index: u32, payload: Vec<u8>) -> Result<(), TcpFrameError> {
        if !self.chunks.contains_key(&chunk_index) {
            self.total_size += payload.len();
            if self.total_size > MAX_MESSAGE_SIZE {
                return Err(TcpFrameError::MessageTooLarge(self.total_size));
            }
            self.chunks.insert(chunk_index, payload);
        }
        Ok(())
    }

    fn is_complete(&self) -> bool {
        self.chunks.len() == self.total_chunks as usize
    }

    fn into_message(self) -> Vec<u8> {
        let mut message = Vec::with_capacity(self.total_size);
        for (_idx, chunk) in self.chunks {
            message.extend_from_slice(&chunk);
        }
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_single_chunk() {
        let message = b"{\"action\":\"get_tasking\"}";
        let framed = encode_message(message);

        assert_eq!(u32::from_be_bytes(framed[0..4].try_into().unwrap()), 32);
        assert_eq!(u32::from_be_bytes(framed[4..8].try_into().unwrap()), 1);
        assert_eq!(u32::from_be_bytes(framed[8..12].try_into().unwrap()), 0);

        let mut decoder = TcpFrameDecoder::new();
        let messages = decoder.push(&framed).unwrap();
        assert_eq!(messages, vec![message.to_vec()]);
    }

    #[test]
    fn handles_split_tcp_reads() {
        let framed = encode_message(b"abcdef");
        let mut decoder = TcpFrameDecoder::new();

        assert!(decoder.push(&framed[..3]).unwrap().is_empty());
        assert!(decoder.push(&framed[3..9]).unwrap().is_empty());
        let messages = decoder.push(&framed[9..]).unwrap();

        assert_eq!(messages, vec![b"abcdef".to_vec()]);
    }

    #[test]
    fn handles_multiple_messages_in_one_read() {
        let mut framed = encode_message(b"one");
        framed.extend_from_slice(&encode_message(b"two"));

        let mut decoder = TcpFrameDecoder::new();
        let messages = decoder.push(&framed).unwrap();

        assert_eq!(messages, vec![b"one".to_vec(), b"two".to_vec()]);
    }

    #[test]
    fn preserves_apollo_extra_empty_chunk_for_exact_multiples() {
        let message = vec![7u8; 8];
        let framed = encode_message_with_chunk_size(&message, 4);

        assert_eq!(u32::from_be_bytes(framed[4..8].try_into().unwrap()), 3);

        let mut decoder = TcpFrameDecoder::new();
        let messages = decoder.push(&framed).unwrap();
        assert_eq!(messages, vec![message]);
    }

    #[test]
    fn rejects_invalid_frame_length() {
        let mut frame = Vec::new();
        frame.extend_from_slice(&1u32.to_be_bytes());
        frame.extend_from_slice(&0u32.to_be_bytes());
        frame.extend_from_slice(&0u32.to_be_bytes());

        let mut decoder = TcpFrameDecoder::new();
        let err = decoder.push(&frame).unwrap_err();
        assert_eq!(err, TcpFrameError::InvalidFrameLength(1));
    }

    #[test]
    fn rejects_out_of_range_chunk_index() {
        let mut frame = Vec::new();
        frame.extend_from_slice(&8u32.to_be_bytes());
        frame.extend_from_slice(&1u32.to_be_bytes());
        frame.extend_from_slice(&1u32.to_be_bytes());

        let mut decoder = TcpFrameDecoder::new();
        let err = decoder.push(&frame).unwrap_err();
        assert_eq!(err, TcpFrameError::InvalidChunkIndex { index: 1, total: 1 });
    }
}
