//! Where the control port's certificate comes from: the operator's own files,
//! or one this mixer makes for itself and keeps.
//!
//! A made certificate is signed by the machine's own certificate authority
//! (`authority.rs`), so a phone that trusts the authority once trusts every
//! certificate made after it, for a new address or after a renewal. It is
//! sealed in the secret store under `control.tls`, the way the channels seal
//! the RTMPS one under `channels.tls`, so the private key never sits in a
//! plain file. The authority's certificate, which is public, is written
//! beside the runtime store for a person to trust on their other machines.
//! A certificate kept from before the authority existed was self signed; it
//! has no `issuer` and is replaced on the first start.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use godwinmix_core::config::ControlTls;
use godwinmix_core::secrets::Secrets;
use godwinmix_core::tls_cert::{self, Pair};

use super::made::made;

/// Where the certificate in use came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Made by this mixer for the names it is reached by, signed by its own
    /// authority. Still `self_signed` on the wire, which is what it is to a
    /// browser that has not been told to trust the authority.
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
    /// The authority that signed a made certificate, as PEM. None for the
    /// operator's own.
    pub authority: Option<String>,
}

/// The certificate for `[control.tls]`.
///
/// `base` is the folder relative paths in the config are read against (the
/// config file's own). `public` is where a made certificate is written for
/// people to download. `names` is what a made one has to cover.
pub fn obtain(tls: &ControlTls, base: &Path, store: &Secrets, public: &Path, names: &[String]) -> Result<Loaded> {
    // An empty string is unset, the way `config.set` clears a path.
    fn given(path: &Option<String>) -> Option<&str> {
        path.as_deref().map(str::trim).filter(|p| !p.is_empty())
    }
    match (given(&tls.cert), given(&tls.key)) {
        (Some(cert), Some(key)) => from_files(&resolve(base, cert), &resolve(base, key)),
        (Some(_), None) | (None, Some(_)) => Err(anyhow!(
            "[control.tls] sets only one of cert and key. Set both to use your own certificate, \
             or remove both to use one this mixer makes."
        )),
        (None, None) => made(store, public, names),
    }
}

/// Where the authority behind a made certificate is written for people to
/// trust: beside the runtime store, `godwinmix.control.crt` for `godwinmix.toml`.
pub fn public_path(config_path: &Path) -> std::path::PathBuf {
    let mut name = config_path.file_stem().unwrap_or_default().to_os_string();
    name.push(".control.crt");
    config_path.with_file_name(name)
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
    Ok(Loaded { pair, source: Source::Files, names: Vec::new(), authority: None })
}

#[cfg(test)]
#[path = "cert_tests.rs"]
mod tests;
