//! The certificate this mixer makes for its control port and keeps: signed
//! by the machine's authority, sealed under `control.tls`, and made again when
//! the names it must cover change, when it nears its end, or when it was not
//! signed by the authority in use (a certificate kept from before there was
//! one was self signed, and is replaced on the first start).

use std::path::Path;

use anyhow::{Context, Result};
use godwinmix_core::secrets::Secrets;
use godwinmix_core::tls_authority;
use godwinmix_core::tls_cert::{self, days_since_epoch, Pair, SERVER_DAYS};
use tracing::info;

use super::authority::{self, Authority};
use super::cert::{Loaded, Source};

/// Where a made certificate is sealed.
const SCOPE: &str = "control.tls";

/// Make a new certificate this many days before the old one runs out.
const RENEW_DAYS: i64 = 30;

/// The kept certificate when it still covers `names` and is not about to run
/// out, otherwise a new one, kept.
pub(super) fn made(store: &Secrets, public: &Path, names: &[String]) -> Result<Loaded> {
    let today = days_since_epoch();
    let ca = authority::obtain(store, today)?;
    let issuer = tls_cert::fingerprint(&ca.pair.cert)?;
    let kept = kept(store).filter(|(_, covered, made, by)| {
        by == &issuer && names.iter().all(|n| covered.contains(n)) && today - made < SERVER_DAYS - RENEW_DAYS
    });
    let (pair, names) = match kept {
        Some((pair, covered, _, _)) => (pair, covered),
        None => (make(store, &ca, &issuer, names, today)?, names.to_vec()),
    };
    if let Err(e) = write_public(public, &ca.pair.cert) {
        tracing::warn!(path = %public.display(), %e, "could not write the certificate authority for people to download");
    }
    authority::publish(&ca.pair.cert);
    Ok(Loaded { pair, source: Source::SelfSigned, names, authority: Some(ca.pair.cert) })
}

/// The kept certificate, its names, the day it was made and the fingerprint
/// of the authority that signed it.
fn kept(store: &Secrets) -> Option<(Pair, Vec<String>, i64, String)> {
    let pair = Pair { cert: store.get(SCOPE, "cert")?, key: store.get(SCOPE, "key")? };
    let names = store.get(SCOPE, "names")?.split(',').map(String::from).collect();
    let made = store.get(SCOPE, "made")?.parse().ok()?;
    let issuer = store.get(SCOPE, "issuer")?;
    tls_cert::check(&pair).ok()?;
    Some((pair, names, made, issuer))
}

fn make(store: &Secrets, ca: &Authority, issuer: &str, names: &[String], today: i64) -> Result<Pair> {
    let label = match super::names::hostname() {
        Some(host) => format!("GodwinMix on {host}"),
        None => "GodwinMix".to_string(),
    };
    let pair = tls_authority::issue_server(&ca.pair, &ca.name, names, &label)?;
    let seal = |field: &str, value: &str| {
        store.set(SCOPE, field, value).with_context(|| {
            "sealing the control port's certificate in the secret store. Check that the \
             secrets folder under GODWINMIX_HOME is writable, or set [control.tls] cert and key"
        })
    };
    seal("cert", &pair.cert)?;
    seal("key", &pair.key)?;
    seal("names", &names.join(","))?;
    seal("made", &today.to_string())?;
    seal("issuer", issuer)?;
    info!(names = %names.join(", "), "made a certificate for HTTPS on the control port");
    Ok(pair)
}

fn write_public(path: &Path, cert: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|on_disk| on_disk == cert) {
        return Ok(());
    }
    std::fs::write(path, cert)
}
