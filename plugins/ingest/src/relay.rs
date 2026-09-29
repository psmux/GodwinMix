//! Handing one stream from the hub to a source in another process.
//!
//! Every mixer source is its own process whose stdout is the media, so a
//! stream the channel server holds has to cross a process boundary to reach
//! one. It crosses on loopback TCP, on the RTMP port itself: a client on this
//! machine that opens with `GMXHUB <app>/<stream>` instead of an RTMP
//! handshake is a hub reader, and gets that stream as FLV.
//!
//! The same port rather than a port per publisher, which is what this module
//! used to open. One listener, no accept thread per stream, and an address
//! that does not change when the plugin restarts, so a source a scene holds
//! across a restart of the mixer still finds its stream.
//!
//! The writer is a hub reader like any other. It blocks on its own socket and
//! on nothing else; a source process that stops reading loses GOPs in its own
//! queue and never slows the publisher.

use std::io::{BufWriter, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::flv;
use crate::hub::{Hub, Recv};

/// What a hub reader says first.
pub const HELLO: &str = "GMXHUB ";

/// The longest request line accepted, so a stray client cannot make this
/// thread buffer without end.
const MAX_LINE: usize = 1024;

/// Serve one reader until its stream ends or it goes away.
pub fn serve(hub: &Hub, mut client: TcpStream, first: &[u8]) -> Result<(), String> {
    let line = read_line(&mut client, first)?;
    let wanted = line
        .strip_prefix(HELLO)
        .ok_or_else(|| "a loopback client that is neither RTMP nor a hub reader".to_string())?;
    let (app, stream) = wanted
        .trim()
        .split_once('/')
        .ok_or_else(|| format!("a hub reader asked for '{wanted}', which is not <app>/<stream>"))?;
    let reader = hub.subscribe(app, stream);
    let mut out = BufWriter::with_capacity(64 * 1024, client.try_clone().map_err(|e| e.to_string())?);
    out.write_all(&flv::header()).map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    loop {
        match reader.recv_timeout(Duration::from_secs(1)) {
            Recv::Tag(tag) => {
                flv::write_to(&mut out, &tag)
                    .and_then(|_| out.flush())
                    .map_err(|e| format!("the reader of {app}/{stream} went away: {e}"))?;
            }
            Recv::Ended => return Ok(()),
            Recv::Timeout if gone(&client) => return Ok(()),
            Recv::Timeout => {}
        }
    }
}

/// Has a reader that is waiting for a publisher hung up? Nothing is being
/// written to it, so a failed write cannot say so.
fn gone(client: &TcpStream) -> bool {
    if client.set_nonblocking(true).is_err() {
        return true;
    }
    let mut probe = [0u8; 1];
    let answer = (&*client).read(&mut probe);
    let _ = client.set_nonblocking(false);
    match answer {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
    }
}

fn read_line(client: &mut TcpStream, first: &[u8]) -> Result<String, String> {
    let mut line = first.to_vec();
    client.set_read_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
    while !line.contains(&b'\n') {
        if line.len() > MAX_LINE {
            return Err("a hub reader sent a request line longer than 1 KiB".into());
        }
        let mut more = [0u8; 256];
        match client.read(&mut more) {
            Ok(0) | Err(_) => return Err("a hub reader hung up before asking for a stream".into()),
            Ok(n) => line.extend_from_slice(&more[..n]),
        }
    }
    client.set_read_timeout(None).map_err(|e| e.to_string())?;
    let end = line.iter().position(|b| *b == b'\n').unwrap_or(line.len());
    Ok(String::from_utf8_lossy(&line[..end]).trim_end_matches('\r').to_string())
}

/// The other end: what an `ingest/rtmp` source calls to read a stream.
pub fn request(address: &str, stream: &str) -> Result<TcpStream, String> {
    let mut socket = TcpStream::connect(address).map_err(|e| {
        format!(
            "could not reach the channel server at {address}: {e}. It is the ingest \
             plugin's RTMP listener; check that ingest/discover is running."
        )
    })?;
    socket
        .write_all(format!("{HELLO}{stream}\n").as_bytes())
        .map_err(|e| format!("could not ask the channel server for {stream}: {e}"))?;
    Ok(socket)
}
