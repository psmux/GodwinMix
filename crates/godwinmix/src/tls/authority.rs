//! The machine's local certificate authority for the control port: made once,
//! sealed in the secret store beside the control port's own key, and kept
//! for ten years, because it is what a phone is told to trust.
//!
//! The authority's private key never leaves the store. Its certificate is
//! public, and is served at `/ca.crt` and written beside the config.

use anyhow::{Context, Result};
use godwinmix_core::secrets::Secrets;
use godwinmix_core::tls_authority::{self, AUTHORITY_DAYS};
use godwinmix_core::tls_cert::{self, Pair};
use tracing::info;

/// Where the authority is sealed. One per secret store, so one per machine.
const SCOPE: &str = "control.ca";

/// Make a new authority this many days before the old one runs out.
const RENEW_DAYS: i64 = 30;

/// The authority, ready to sign with.
#[derive(Debug, Clone)]
pub struct Authority {
    pub pair: Pair,
    /// Its common name, which signing needs exactly as it was made.
    pub name: String,
}

/// The kept authority, or a new one, sealed, when there is none or it is
/// about to run out.
pub fn obtain(store: &Secrets, today: i64) -> Result<Authority> {
    if let Some(found) = kept(store, today) {
        return Ok(found);
    }
    let name = tls_authority::authority_name(super::names::hostname().as_deref());
    let pair = tls_authority::make_authority(&name)?;
    let seal = |field: &str, value: &str| {
        store.set(SCOPE, field, value).with_context(|| {
            "sealing the control port's certificate authority in the secret store. Check that the \
             secrets folder under GODWINMIX_HOME is writable, or set [control.tls] cert and key"
        })
    };
    seal("cert", &pair.cert)?;
    seal("key", &pair.key)?;
    seal("name", &name)?;
    seal("made", &today.to_string())?;
    let print = tls_cert::fingerprint(&pair.cert).unwrap_or_default();
    info!(%name, fingerprint = %print, "made a certificate authority for HTTPS on the control port");
    Ok(Authority { pair, name })
}

/// The authority's certificate this process serves at `/ca.crt`, set once at
/// startup when the control port's certificate is one this mixer made.
static SERVED: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn publish(pem: &str) {
    let _ = SERVED.set(pem.to_string());
}

/// What `/ca.crt` answers with, if anything.
pub fn served() -> Option<&'static str> {
    SERVED.get().map(String::as_str)
}

/// The second line a person who just started the mixer reads.
pub fn announce(fingerprint: &str, url: &str) {
    eprintln!("To trust it on a phone, install its authority from {url}ca.crt (fingerprint {fingerprint}).");
}

fn kept(store: &Secrets, today: i64) -> Option<Authority> {
    let pair = Pair { cert: store.get(SCOPE, "cert")?, key: store.get(SCOPE, "key")? };
    let name = store.get(SCOPE, "name")?;
    let made: i64 = store.get(SCOPE, "made")?.parse().ok()?;
    (today - made < AUTHORITY_DAYS - RENEW_DAYS).then_some(Authority { pair, name })
}
