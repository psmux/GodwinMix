//! HTTPS on the control port.
//!
//! A browser only gives a page the camera and microphone in a secure
//! context: `https://`, or `http://localhost`. An operator on another laptop
//! opening `http://192.168.1.20:8080` gets neither, and the browser does not
//! say why. So the control port answers HTTPS itself.
//!
//! One port, both protocols. Each new connection is looked at once: a first
//! byte of 0x16 is a TLS handshake, and anything else is plain HTTP, since
//! every HTTP method starts with a capital letter (`listen.rs`). The other
//! shape was a second port for HTTPS. It was turned down because one port
//! keeps `http://localhost:8080` and every script that uses it unchanged,
//! needs no second firewall rule or port forward, and lets the address a
//! person already knows work with `https://` in front of it. The cost is one
//! peeked byte per connection, not per request.
//!
//! Everything above the listener is the same router either way, so the
//! WebSocket upgrades (`/ws`, `/rpc`), WHIP and WHEP all work over TLS with
//! nothing of their own. Certificates are found or made once at startup,
//! never on a request, and never on a GStreamer thread.
//!
//! The certificate is the operator's own (`[control.tls] cert` and `key`) or
//! one made here and kept (`cert.rs`). ACME is the third source, still to
//! come; the seam is `cert::Source`.

pub mod authority;
pub mod cert;
pub mod listen;
mod made;
pub mod names;

use std::path::Path;
use std::sync::{Arc, OnceLock};

use anyhow::Result;
use godwinmix_core::config::Config;
use godwinmix_core::tls_cert;
use godwinmix_protocol::types::TlsInfo;
pub use cert::public_path;
pub use listen::Sniffing;
use tokio_rustls::TlsAcceptor;

/// What `core.info` says about HTTPS, set once at startup.
static INFO: OnceLock<TlsInfo> = OnceLock::new();

/// How a station tells each show what its port answers HTTPS with. A show
/// answers `core.info` but binds only loopback; the port a person opens is
/// the station's.
pub const INFO_ENV: &str = "GODWINMIX_CONTROL_TLS";

/// HTTPS on the control port, when it is on and started: this process's own,
/// or, in a show, the station's.
pub fn info() -> Option<TlsInfo> {
    static FROM_STATION: OnceLock<Option<TlsInfo>> = OnceLock::new();
    INFO.get().cloned().or_else(|| {
        FROM_STATION
            .get_or_init(|| std::env::var(INFO_ENV).ok().and_then(|json| serde_json::from_str(&json).ok()))
            .clone()
    })
}

/// Serve `app` on `listener`, plain HTTP alone or, with `tls`, HTTP and HTTPS
/// on the same port. Peer addresses reach handlers as `ConnectInfo` either way.
pub async fn serve(listener: tokio::net::TcpListener, app: axum::Router, tls: Option<TlsAcceptor>) -> std::io::Result<()> {
    let app = app.into_make_service_with_connect_info::<std::net::SocketAddr>();
    match tls {
        None => axum::serve(listener, app).await,
        Some(acceptor) => {
            use axum::serve::ListenerExt;
            // `tap_io` does nothing to the stream. It is there because axum
            // passes the peer address on as connect info for its own
            // TcpListener, and for any listener only once wrapped like this.
            let sniffing = Sniffing::new(listener, acceptor)?.tap_io(|_| {});
            axum::serve(sniffing, app).await
        }
    }
}

/// What the listener needs and what a client is told.
pub struct Serving {
    pub acceptor: TlsAcceptor,
    pub info: TlsInfo,
}

/// Find or make the certificate for `[control.tls]` and build the acceptor.
/// None when HTTPS is switched off. An error says what is wrong with the
/// configured certificate and what to do about it; the caller serves plain
/// HTTP and says so, rather than leave nobody able to reach the mixer.
pub fn prepare(cfg: &Config, bind: &str, config_path: &Path) -> Result<Option<Serving>> {
    let tls = &cfg.control.tls;
    if !tls.enabled {
        return Ok(None);
    }
    let base = config_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let wanted = names::for_machine(bind);
    let store = crate::control::methods::plugins::secrets();
    let loaded = cert::obtain(tls, base, store, &public_path(config_path), &wanted)?;
    let serving = serving(&loaded, bind)?;
    let _ = INFO.set(serving.info.clone());
    Ok(Some(serving))
}

/// The acceptor and the `core.info` record for a loaded certificate.
pub fn serving(loaded: &cert::Loaded, bind: &str) -> Result<Serving> {
    let mut config = tls_cert::server_config(&loaded.pair)?;
    // HTTP/1.1 only: axum here is built without HTTP/2, and a WebSocket
    // upgrade is an HTTP/1.1 thing.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let url_names = match loaded.source {
        cert::Source::SelfSigned => loaded.names.clone(),
        cert::Source::Files => names::for_machine(bind),
    };
    let info = TlsInfo {
        source: loaded.source.as_str().into(),
        fingerprint: tls_cert::fingerprint(&loaded.pair.cert)?,
        names: loaded.names.clone(),
        urls: names::urls(bind, &url_names),
        authority: loaded.authority.as_deref().map(tls_cert::fingerprint).transpose()?,
    };
    Ok(Serving { acceptor: TlsAcceptor::from(Arc::new(config)), info })
}

/// [`prepare`] at startup, with what goes wrong told to the operator as an
/// alert and on stderr. The mixer then serves plain HTTP, so it can still be
/// reached to fix the setting.
pub fn start(cfg: &Config, bind: &str, config_path: &Path, mixer: &godwinmix_core::mixer::MixerHandle) -> Option<Serving> {
    match prepare(cfg, bind, config_path) {
        Ok(serving) => serving,
        Err(e) => {
            let message = format!("HTTPS is off on the control port, plain HTTP still works: {e:#}");
            tracing::error!(%message, "control port TLS");
            eprintln!("{message}");
            mixer.publish_alert(godwinmix_core::state::Severity::Error, message);
            None
        }
    }
}

/// The line a person who just started the mixer reads.
pub fn announce(info: &TlsInfo) {
    let Some(first) = info.urls.first() else { return };
    eprintln!("HTTPS is on the same port: {first} (certificate fingerprint {}).", info.fingerprint);
    if let Some(print) = &info.authority {
        authority::announce(print, first);
    }
}
