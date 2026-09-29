//! The socket under an RTMP link: plain TCP, or TLS for `rtmps://`.
//!
//! TLS is rustls over the `ring` provider with the Mozilla roots compiled in,
//! all three already in the workspace under the core's node bridge and
//! reqwest. Facebook takes nothing but RTMPS, so without this the one
//! platform most churches stream to would be missing.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use super::link::Failure;
use super::target::RtmpUrl;

/// How long a far end has to answer a connect.
const CONNECT: Duration = Duration::from_secs(5);
/// How long one write may block before the far end counts as gone. The queue
/// in front drops GOPs long before this, so the publisher never sees it.
const STALL: Duration = Duration::from_secs(10);

pub enum Io {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Io {
    /// Connect, and for `rtmps://` wrap the socket in TLS.
    pub fn open(url: &RtmpUrl) -> Result<Io, Failure> {
        let at = format!("{}://{}:{}", if url.tls { "rtmps" } else { "rtmp" }, url.host, url.port);
        let addrs = (url.host.as_str(), url.port).to_socket_addrs().map_err(|_| {
            Failure::Lost(format!(
                "could not find {}. Check the server address, and that this machine is online",
                url.host
            ))
        })?;
        let tcp = addrs
            .filter_map(|a| TcpStream::connect_timeout(&a, CONNECT).ok())
            .next()
            .ok_or_else(|| Failure::Lost(format!("nothing answered at {at}")))?;
        let _ = tcp.set_nodelay(true);
        let _ = tcp.set_write_timeout(Some(STALL));
        let _ = tcp.set_read_timeout(Some(STALL));
        if !url.tls {
            return Ok(Io::Plain(tcp));
        }
        let name = ServerName::try_from(url.host.clone())
            .map_err(|_| Failure::Refused(format!("'{}' is not a server name TLS can check", url.host)))?;
        let conn = ClientConnection::new(tls_config(), name)
            .map_err(|e| Failure::Lost(format!("the secure connection to {at} failed: {e}")))?;
        Ok(Io::Tls(Box::new(StreamOwned::new(conn, tcp))))
    }

    fn tcp(&self) -> &TcpStream {
        match self {
            Io::Plain(s) => s,
            Io::Tls(s) => s.get_ref(),
        }
    }

    /// How long a read waits. Short while publishing, so reading what the
    /// server says never holds up the next tag.
    pub fn read_wait(&self, wait: Duration) {
        let _ = self.tcp().set_read_timeout(Some(wait));
    }

    pub fn shutdown(&self) {
        let _ = self.tcp().shutdown(std::net::Shutdown::Both);
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

/// Is this error a read that timed out, rather than a connection that died?
pub fn timed_out(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
}

fn tls_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .expect("ring supports the default TLS versions")
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}
