//! One port, two protocols: a listener that looks at the first byte of each
//! connection and hands axum either the plain stream or a TLS one.
//!
//! The handshake is never done in `accept`. A task per connection waits for
//! the first byte and, for TLS, the handshake, each under a timeout, and
//! passes the finished stream over a channel. So a client that connects and
//! says nothing, or stalls halfway through a handshake, holds up nobody else.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use tracing::debug;

/// The first byte of a TLS record carrying a handshake. No HTTP method starts
/// with it, so one byte is enough to tell the two apart.
const HANDSHAKE: u8 = 0x16;

/// How long a connection may sit before its first byte. Browsers open
/// connections ahead of need and sometimes leave them quiet for a while.
const FIRST_BYTE: Duration = Duration::from_secs(30);

/// How long a TLS handshake may take once it has started.
const HANDSHAKE_TIME: Duration = Duration::from_secs(10);

/// A connection, plain or encrypted. Both read and write the same way, so
/// everything above this (the router, the WebSocket upgrade, WHIP and WHEP)
/// does not know which one it has.
pub enum Conn {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

/// The listener `axum::serve` is handed when HTTPS is on.
pub struct Sniffing {
    ready: mpsc::Receiver<(Conn, SocketAddr)>,
    local: SocketAddr,
}

impl Sniffing {
    /// Take over `listener`. The accept loop runs on its own task until this
    /// is dropped.
    pub fn new(listener: TcpListener, acceptor: TlsAcceptor) -> io::Result<Self> {
        let local = listener.local_addr()?;
        let (tx, ready) = mpsc::channel(64);
        tokio::spawn(accept_loop(listener, acceptor, tx));
        Ok(Self { ready, local })
    }
}

async fn accept_loop(listener: TcpListener, acceptor: TlsAcceptor, tx: mpsc::Sender<(Conn, SocketAddr)>) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(e) => {
                // Out of file descriptors, mostly. The same pause axum makes.
                debug!(%e, "accept failed");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        if tx.is_closed() {
            return;
        }
        let (acceptor, tx) = (acceptor.clone(), tx.clone());
        tokio::spawn(async move {
            match classify(stream, &acceptor).await {
                Ok(conn) => {
                    let _ = tx.send((conn, peer)).await;
                }
                Err(e) => debug!(%peer, %e, "connection dropped before it was served"),
            }
        });
    }
}

/// Plain or TLS, by the first byte.
async fn classify(stream: TcpStream, acceptor: &TlsAcceptor) -> io::Result<Conn> {
    let mut first = [0u8; 1];
    let peeked = tokio::time::timeout(FIRST_BYTE, stream.peek(&mut first))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "nothing sent"))??;
    if peeked == 0 || first[0] != HANDSHAKE {
        return Ok(Conn::Plain(stream));
    }
    let tls = tokio::time::timeout(HANDSHAKE_TIME, acceptor.accept(stream))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "TLS handshake took too long"))??;
    Ok(Conn::Tls(Box::new(tls)))
}

impl axum::serve::Listener for Sniffing {
    type Io = Conn;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Conn, SocketAddr) {
        match self.ready.recv().await {
            Some(ready) => ready,
            // The accept loop only ends when this end is gone, so this is
            // never reached while axum is still asking.
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local)
    }
}

impl AsyncRead for Conn {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Conn {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_flush(cx),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}
