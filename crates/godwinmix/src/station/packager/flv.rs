//! A hub reader on the ingest plugin's relay: `GMXHUB <app>/<stream>` on
//! its loopback port, and FLV tags back.
//!
//! The relay writes the FLV header, then the stream's headers, then tags
//! from the next keyframe on (`plugins/ingest/src/relay.rs`). Tags are read
//! whole off a buffered socket on the packager's own thread; nothing else
//! waits on it.

use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// The largest tag taken, so a stray stream cannot make this allocate
/// without end. A keyframe of 4K HEVC is well under it.
const MAX_TAG: usize = 16 * 1024 * 1024;
/// How long the relay has to answer a reader with the FLV header.
const HELLO_WAIT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Audio,
    Video,
    Script,
}

/// One tag: what it is, its time in milliseconds and its body.
#[derive(Debug, Clone)]
pub struct Tag {
    pub kind: Kind,
    pub ms: u32,
    pub body: Vec<u8>,
}

pub struct Reader {
    io: BufReader<TcpStream>,
}

impl Reader {
    /// Ask the relay at `relay` for `app/stream`. `wait` bounds the connect
    /// and every read after it, so a quiet stream wakes the caller to look
    /// at its stop flag.
    pub fn open(relay: SocketAddr, path: &str, wait: Duration) -> std::io::Result<Reader> {
        let mut sock = TcpStream::connect_timeout(&relay, wait)?;
        // The relay's first answer can take a couple of seconds on a busy
        // machine (seen at 2.3 s with several mixers running), so the header
        // gets longer than a read between tags does.
        sock.set_read_timeout(Some(wait.max(HELLO_WAIT)))?;
        sock.set_nodelay(true)?;
        sock.write_all(format!("GMXHUB {path}\n").as_bytes())?;
        let mut io = BufReader::with_capacity(256 * 1024, sock);
        let mut header = [0u8; 13];
        io.read_exact(&mut header)?;
        if &header[..3] != b"FLV" {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the relay did not answer with FLV"));
        }
        io.get_ref().set_read_timeout(Some(wait))?;
        Ok(Reader { io })
    }

    /// The next tag. A timeout is `WouldBlock` or `TimedOut`, and the
    /// stream ending is `UnexpectedEof`.
    pub fn next(&mut self) -> std::io::Result<Tag> {
        read_tag(&mut self.io)
    }
}

/// One tag and the size field after it.
pub fn read_tag(io: &mut impl Read) -> std::io::Result<Tag> {
    let mut head = [0u8; 11];
    io.read_exact(&mut head)?;
    let size = (usize::from(head[1]) << 16) | (usize::from(head[2]) << 8) | usize::from(head[3]);
    if size > MAX_TAG {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("a tag of {size} bytes")));
    }
    let ms = (u32::from(head[7]) << 24) | (u32::from(head[4]) << 16) | (u32::from(head[5]) << 8) | u32::from(head[6]);
    let mut body = vec![0u8; size + 4];
    io.read_exact(&mut body)?;
    body.truncate(size);
    let kind = match head[0] & 0x1f {
        8 => Kind::Audio,
        9 => Kind::Video,
        _ => Kind::Script,
    };
    Ok(Tag { kind, ms, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(kind: u8, ms: u32, body: &[u8]) -> Vec<u8> {
        let n = body.len();
        let mut out = vec![kind, (n >> 16) as u8, (n >> 8) as u8, n as u8, (ms >> 16) as u8, (ms >> 8) as u8, ms as u8, (ms >> 24) as u8, 0, 0, 0];
        out.extend_from_slice(body);
        out.extend_from_slice(&((n + 11) as u32).to_be_bytes());
        out
    }

    #[test]
    fn tags_read_back_with_their_kind_time_and_body() {
        let mut bytes = tag(9, 0x0100_0203, &[0x17, 1, 0, 0, 0, 9]);
        bytes.extend(tag(8, 40, &[0xaf, 1, 7]));
        let mut io = std::io::Cursor::new(bytes);
        let v = read_tag(&mut io).unwrap();
        assert_eq!((v.kind, v.ms, v.body.len()), (Kind::Video, 0x0100_0203, 6));
        let a = read_tag(&mut io).unwrap();
        assert_eq!((a.kind, a.ms, a.body), (Kind::Audio, 40, vec![0xaf, 1, 7]));
        assert_eq!(read_tag(&mut io).unwrap_err().kind(), std::io::ErrorKind::UnexpectedEof);
    }
}
