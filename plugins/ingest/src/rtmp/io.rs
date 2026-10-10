//! What an RTMP connection reads and writes: the socket itself, or TLS over
//! it for RTMPS.
//!
//! TLS ends here, in the plugin, with rustls: the crate the restreamer already
//! uses to reach Facebook, so RTMPS in costs no new dependency. Behind it the
//! connection is the same RTMP session feeding the same hub.

use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use crate::proto::Tls;

pub enum Io {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ServerConnection, TcpStream>>),
}

impl Io {
    /// Wrap an accepted socket, in TLS when the listener has a certificate.
    pub fn over(socket: TcpStream, tls: Option<&Arc<ServerConfig>>) -> Result<Io, String> {
        let Some(config) = tls else { return Ok(Io::Plain(socket)) };
        let session = ServerConnection::new(config.clone()).map_err(|e| format!("TLS would not start: {e}"))?;
        Ok(Io::Tls(Box::new(StreamOwned::new(session, socket))))
    }

    pub fn is_plain(&self) -> bool {
        matches!(self, Io::Plain(_))
    }
}

/// How long one read waits before it looks at the stop flag and the clock.
pub const TICK: Duration = Duration::from_secs(1);

/// How long a connection may send nothing before it is taken to be gone. A
/// pulled cable or a phone that left the Wi-Fi sends no FIN, and a read
/// with no timeout waits on it for as long as the operating system keeps
/// the socket, which is minutes or hours. Every working publisher sends
/// sound every 20 to 40 ms and a picture every frame, so five seconds of
/// nothing is a network that has gone. Freeing the connection then is what
/// lets the same encoder's reconnect in.
pub const IDLE: Duration = Duration::from_secs(5);

/// What one read found.
pub enum Got {
    Bytes(usize),
    /// The peer closed the connection, or it was cut off.
    Closed,
    /// Nothing in the last [`TICK`], and not yet [`IDLE`].
    Nothing,
}

impl Io {
    /// Put the read and write timeouts on the socket under a connection.
    pub fn time_out(socket: &TcpStream) {
        let _ = socket.set_read_timeout(Some(TICK));
        let _ = socket.set_write_timeout(Some(IDLE));
    }

    /// Read once. `last` is when bytes last came, and moves when they do.
    /// An error, [`IDLE`] of silence among them, is the sentence to log.
    pub fn read_some(&mut self, buf: &mut [u8], last: &mut Instant, idle: Duration) -> Result<Got, String> {
        match self.read(buf) {
            Ok(0) => Ok(Got::Closed),
            Ok(n) => {
                *last = Instant::now();
                Ok(Got::Bytes(n))
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if last.elapsed() < idle {
                    return Ok(Got::Nothing);
                }
                Err(format!(
                    "nothing came for {} s, so the network to the publisher has gone. The \
                     connection is closed, and the same encoder reconnecting is let in.",
                    idle.as_secs()
                ))
            }
            Err(e) => Err(format!("reading from the publisher failed: {e}")),
        }
    }
}

impl Read for Io {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Io::Plain(s) => s.read(buf),
            Io::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Io {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Io::Plain(s) => s.write(buf),
            Io::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Io::Plain(s) => s.flush(),
            Io::Tls(s) => s.flush(),
        }
    }
}

/// A rustls server configuration from the PEM the core handed over. A
/// certificate that will not load is refused with a sentence the Channels
/// page can show next to its upload button.
pub fn server_config(tls: &Tls) -> Result<Arc<ServerConfig>, String> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(tls.cert.as_bytes())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("the RTMPS certificate is not PEM this can read: {e}. Upload it again, or make a self signed one."))?;
    if certs.is_empty() {
        return Err("the RTMPS certificate file has no certificate in it. Upload the .crt or .pem your certificate came in.".into());
    }
    let key = PrivateKeyDer::from_pem_slice(tls.key.as_bytes())
        .map_err(|e| format!("the RTMPS private key is not PEM this can read: {e}. Upload the key that came with the certificate."))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS is not available in this build: {e}"))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("the RTMPS certificate and key do not go together: {e}. Upload the key that belongs to this certificate."))?;
    Ok(Arc::new(config))
}
