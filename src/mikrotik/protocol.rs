use bytes::{Buf, BufMut, Bytes, BytesMut};
use encoding_rs::WINDOWS_1252;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error")]
    Parse,
}

pub fn encode_length(len: usize) -> Bytes {
    let mut buf = BytesMut::new();
    if len < 0x80 {
        buf.put_u8(len as u8);
    } else if len < 0x4000 {
        let l = (len as u32) | 0x8000;
        buf.put_u16(l as u16);
    } else if len < 0x200000 {
        let l = (len as u32) | 0xc00000;
        buf.put_u8(((l >> 16) & 0xff) as u8);
        buf.put_u8(((l >> 8) & 0xff) as u8);
        buf.put_u8((l & 0xff) as u8);
    } else if len < 0x10000000 {
        let l = (len as u32) | 0xe0000000;
        buf.put_u32(l);
    } else {
        buf.put_u8(0xf0);
        buf.put_u32(len as u32);
    }
    buf.freeze()
}

pub fn decode_length(buf: &[u8]) -> Option<(usize, usize)> {
    if buf.is_empty() {
        return None;
    }
    let mut idx = 0;
    let b = buf[idx];
    idx += 1;
    if b & 0x80 == 0 {
        return Some((b as usize, idx));
    }
    if (b & 0xc0) == 0x80 {
        if buf.len() < 2 {
            return None;
        }
        let len = ((b & 0x3f) as usize) << 8 | buf[1] as usize;
        return Some((len, 2));
    }
    if (b & 0xe0) == 0xc0 {
        if buf.len() < 3 {
            return None;
        }
        let mut len = ((b & 0x1f) as usize) << 8 | buf[1] as usize;
        len = (len << 8) | buf[2] as usize;
        return Some((len, 3));
    }
    if (b & 0xf0) == 0xe0 {
        if buf.len() < 4 {
            return None;
        }
        let mut len = ((b & 0x0f) as usize) << 8 | buf[1] as usize;
        len = (len << 8) | buf[2] as usize;
        len = (len << 8) | buf[3] as usize;
        return Some((len, 4));
    }
    if b == 0xf0 {
        if buf.len() < 5 {
            return None;
        }
        let len = ((buf[1] as usize) << 24) | ((buf[2] as usize) << 16) | ((buf[3] as usize) << 8) | (buf[4] as usize);
        return Some((len, 5));
    }
    Some((b as usize, 1))
}

pub fn encode_sentence(s: Option<&str>) -> Bytes {
    match s {
        None => Bytes::from_static(&[0x00]),
        Some(str) => {
            let (encoded, _, _) = WINDOWS_1252.encode(str);
            let mut buf = BytesMut::new();
            buf.extend_from_slice(&encode_length(encoded.len()));
            buf.extend_from_slice(&encoded);
            buf.freeze()
        }
    }
}
