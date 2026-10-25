//! RouterOS API wire format: a sentence is a series of words, each prefixed by
//! its length, closed by an empty word.

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Largest word accepted from the router. Real replies are tiny; this only stops
/// a bad length from allocating gigabytes.
const MAX_WORD: usize = 1 << 20;

pub fn encode_len(len: usize) -> Vec<u8> {
    match len {
        0..=0x7F => vec![len as u8],
        0x80..=0x3FFF => ((len as u16) | 0x8000).to_be_bytes().to_vec(),
        0x4000..=0x1F_FFFF => ((len as u32) | 0xC0_0000).to_be_bytes()[1..].to_vec(),
        0x20_0000..=0x0FFF_FFFF => ((len as u32) | 0xE000_0000).to_be_bytes().to_vec(),
        _ => {
            let mut out = vec![0xF0];
            out.extend_from_slice(&(len as u32).to_be_bytes());
            out
        }
    }
}

pub async fn write_sentence<W: AsyncWrite + Unpin>(w: &mut W, words: &[Vec<u8>]) -> io::Result<()> {
    let mut buf = Vec::new();
    for word in words {
        buf.extend(encode_len(word.len()));
        buf.extend_from_slice(word);
    }
    buf.push(0);
    w.write_all(&buf).await?;
    w.flush().await
}

async fn read_len<R: AsyncRead + Unpin>(r: &mut R) -> io::Result<usize> {
    let first = r.read_u8().await?;
    let extra = match first {
        0x00..=0x7F => return Ok(first as usize),
        0x80..=0xBF => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0 => 4,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "reserved control byte in RouterOS word length",
            ));
        }
    };
    let mut len = match first {
        0x80..=0xBF => (first & 0x3F) as usize,
        0xC0..=0xDF => (first & 0x1F) as usize,
        0xE0..=0xEF => (first & 0x0F) as usize,
        _ => 0,
    };
    for _ in 0..extra {
        len = (len << 8) | r.read_u8().await? as usize;
    }
    Ok(len)
}

/// Reads one sentence: words up to the empty terminator.
pub async fn read_sentence<R: AsyncRead + Unpin>(r: &mut R) -> io::Result<Vec<Vec<u8>>> {
    let mut words = Vec::new();
    loop {
        let len = read_len(r).await?;
        if len == 0 {
            return Ok(words);
        }
        if len > MAX_WORD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "RouterOS word too long",
            ));
        }
        let mut word = vec![0; len];
        r.read_exact(&mut word).await?;
        words.push(word);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Boundaries from the MikroTik API documentation.
    #[test]
    fn encodes_lengths_at_every_boundary() {
        assert_eq!(encode_len(0), [0x00]);
        assert_eq!(encode_len(0x7F), [0x7F]);
        assert_eq!(encode_len(0x80), [0x80, 0x80]);
        assert_eq!(encode_len(0x3FFF), [0xBF, 0xFF]);
        assert_eq!(encode_len(0x4000), [0xC0, 0x40, 0x00]);
        assert_eq!(encode_len(0x1F_FFFF), [0xDF, 0xFF, 0xFF]);
        assert_eq!(encode_len(0x20_0000), [0xE0, 0x20, 0x00, 0x00]);
        assert_eq!(encode_len(0x0FFF_FFFF), [0xEF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(encode_len(0x1000_0000), [0xF0, 0x10, 0x00, 0x00, 0x00]);
    }

    #[tokio::test]
    async fn decodes_what_it_encodes() {
        for len in [1usize, 0x7F, 0x80, 0x3FFF, 0x4000, 0x1F_FFFF] {
            let bytes = encode_len(len);
            let mut cursor = std::io::Cursor::new(bytes);
            assert_eq!(read_len(&mut cursor).await.unwrap(), len);
        }
    }

    #[tokio::test]
    async fn round_trips_a_sentence_with_a_long_word() {
        let words = vec![b"/ip/arp/print".to_vec(), vec![b'x'; 300]];
        let mut buf = Vec::new();
        write_sentence(&mut buf, &words).await.unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        assert_eq!(read_sentence(&mut cursor).await.unwrap(), words);
    }

    #[tokio::test]
    async fn reads_a_sentence_delivered_one_byte_at_a_time() {
        let mut buf = Vec::new();
        write_sentence(&mut buf, &[b"!done".to_vec(), b"=ret=*1".to_vec()])
            .await
            .unwrap();
        let (mut tx, mut rx) = tokio::io::duplex(1);
        let writer = tokio::spawn(async move {
            tx.write_all(&buf).await.unwrap();
        });
        let words = read_sentence(&mut rx).await.unwrap();
        writer.await.unwrap();
        assert_eq!(words, [b"!done".to_vec(), b"=ret=*1".to_vec()]);
    }

    #[tokio::test]
    async fn rejects_reserved_control_bytes_and_oversized_words() {
        let mut reserved = std::io::Cursor::new(vec![0xF8]);
        assert!(read_sentence(&mut reserved).await.is_err());
        let mut huge = std::io::Cursor::new(encode_len(MAX_WORD + 1));
        assert!(read_sentence(&mut huge).await.is_err());
    }

    #[tokio::test]
    async fn truncated_input_is_an_error() {
        let mut cursor = std::io::Cursor::new(vec![5, b'a', b'b']);
        assert!(read_sentence(&mut cursor).await.is_err());
    }
}
