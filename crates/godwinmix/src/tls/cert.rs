//! Where the control port's certificate comes from: the operator's own files,
//! or one this mixer makes for itself and keeps.
//!
//! A made certificate is sealed in the secret store under `control.tls`, the
//! way the channels seal the RTMPS one under `channels.tls`, so the private
//! key never sits in a plain file. The certificate alone, which is public, is
//! also written beside the runtime store, for a person who wants to trust it
//! on their other machines instead of clicking through a warning.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use godwinmix_core::config::ControlTls;
use godwinmix_core::secrets::Secrets;
use godwinmix_core::tls_cert::{self, Pair, SERVER_DAYS};
use tracing::info;

/// Where a made certificate is sealed.
const SCOPE: &str = "control.tls";

/// Make a new certificate this many days before the old one runs out.
const RENEW_DAYS: i64 = 30;

/// Where the certificate in use came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Made by this mixer for the names it is reached by.
    SelfSigned,
    /// The operator's own, from `[control.tls] cert` and `key`.
    Files,
    // ACME goes here: a third arm, `Acme`, filled by an issuer that answers
    // the challenge and renews the pair before it runs out. `obtain` picks
    // it when a future `[control.tls] acme` names a domain; everything after
    // `obtain` (the listener, `core.info`) takes any `Loaded` the same way.
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::SelfSigned => "self_signed",
            Source::Files => "files",
        }
    }
}

/// A certificate ready to serve.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub pair: Pair,
    pub source: Source,
    /// The names a made certificate covers. Empty for the operator's own.
    pub names: Vec<String>,
}

/// The certificate for `[control.tls]`.
///
/// `base` is the folder relative paths in the config are read against (the
/// config file's own). `public` is where a made certificate is written for
/// people to download. `names` is what a made one has to cover.
pub fn obtain(tls: &ControlTls, base: &Path, store: &Secrets, public: &Path, names: &[String]) -> Result<Loaded> {
    match (&tls.cert, &tls.key) {
        (Some(cert), Some(key)) => from_files(&resolve(base, cert), &resolve(base, key)),
        (Some(_), None) | (None, Some(_)) => Err(anyhow!(
            "[control.tls] sets only one of cert and key. Set both to use your own certificate, \
             or remove both to use one this mixer makes."
        )),
        (None, None) => made(store, public, names),
    }
}

fn resolve(base: &Path, path: &str) -> PathBuf {
    let path = godwinmix_host::home::expand(path);
    if path.is_relative() { base.join(path) } else { path }
}

fn from_files(cert: &Path, key: &Path) -> Result<Loaded> {
    let read = |path: &Path, what: &str| {
        std::fs::read_to_string(path).map_err(|e| {
            anyhow!(
                "[control.tls] {what} is {}, which cannot be read ({e}). Fix the path, or remove \
                 cert and key to use a certificate this mixer makes.",
                path.display()
            )
        })
    };
    let pair = Pair { cert: read(cert, "cert")?, key: read(key, "key")? };
    tls_cert::check(&pair).with_context(|| format!("[control.tls] cert {} and key {}", cert.display(), key.display()))?;
    Ok(Loaded { pair, source: Source::Files, names: Vec::new() })
}

/// The kept certificate when it still covers `names` and is not about to run
/// out, otherwise a new one, kept.
fn made(store: &Secrets, public: &Path, names: &[String]) -> Result<Loaded> {
    let today = days_now();
    let kept = kept(store).filter(|(_, covered, made)| names.iter().all(|n| covered.contains(n)) && today - made < SERVER_DAYS - RENEW_DAYS);
    let (pair, names) = match kept {
        Some((pair, covered, _)) => (pair, covered),
        None => (make(store, names, today)?, names.to_vec()),
    };
    if let Err(e) = write_public(public, &pair.cert) {
        tracing::warn!(path = %public.display(), %e, "could not write the certificate for people to download");
    }
    Ok(Loaded { pair, source: Source::SelfSigned, names })
}

fn kept(store: &Secrets) -> Option<(Pair, Vec<String>, i64)> {
    let pair = Pair { cert: store.get(SCOPE, "cert")?, key: store.get(SCOPE, "key")? };
    let names = store.get(SCOPE, "names")?.split(',').map(String::from).collect();
    let made = store.get(SCOPE, "made")?.parse().ok()?;
    tls_cert::check(&pair).ok()?;
    Some((pair, names, made))
}

fn make(store: &Secrets, names: &[String], today: i64) -> Result<Pair> {
    let label = match super::names::hostname() {
        Some(host) => format!("GodwinMix on {host}"),
        None => "GodwinMix".to_string(),
    };
    let pair = tls_cert::for_server(names, &label)?;
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
    info!(names = %names.join(", "), "made a certificate for HTTPS on the control port");
    Ok(pair)
}

fn write_public(path: &Path, cert: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|on_disk| on_disk == cert) {
        return Ok(());
    }
    std::fs::write(path, cert)
}

fn days_now() -> i64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (secs / 86_400) as i64
}

#[cfg(test)]
#[path = "cert_tests.rs"]
mod tests;
