//! Wire format: `u32` little-endian length + bincode body.
use serde::de::DeserializeOwned;
use serde::Serialize;

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    let body = bincode::serialize(msg).expect("serialize");
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

#[derive(Debug, PartialEq)]
pub enum FrameError {
    /// Declared length above the limit: refused before allocating it.
    TooBig,
    Malformed,
}

/// Incremental frame decoder for a byte stream.
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn next<T: DeserializeOwned>(&mut self, max: usize) -> Option<Result<T, FrameError>> {
        if self.buf.len() < 4 {
            return None;
        }
        let len = u32::from_le_bytes(self.buf[..4].try_into().unwrap()) as usize;
        if len > max {
            return Some(Err(FrameError::TooBig));
        }
        if self.buf.len() < 4 + len {
            return None;
        }
        let msg = bincode::deserialize::<T>(&self.buf[4..4 + len]).map_err(|_| FrameError::Malformed);
        self.buf.drain(..4 + len);
        Some(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::C2S;

    #[test]
    fn roundtrip_in_pieces() {
        let bytes = encode(&C2S::Say { text: "hola".into() });
        let mut r = FrameReader::default();
        r.push(&bytes[..3]);
        assert!(r.next::<C2S>(512).is_none());
        r.push(&bytes[3..]);
        assert!(matches!(r.next::<C2S>(512), Some(Ok(C2S::Say { .. }))));
    }

    #[test]
    fn oversized_is_refused() {
        let mut r = FrameReader::default();
        r.push(&u32::MAX.to_le_bytes());
        assert_eq!(r.next::<C2S>(512).map(|x| x.err()), Some(Some(FrameError::TooBig)));
    }
}
