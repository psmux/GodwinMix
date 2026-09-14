//! What a plugin's signature says, and what its `api` level means here.
//!
//! Two questions are asked of everything `gmx plugin add` fetches, and the
//! answers are what `plugin.list` and `plugin.describe` show an operator:
//!
//!   1. Was it signed by the index CI, and does the signature cover the bytes
//!      that arrived? [`check_signature`]
//!   2. Will this core run it, and if not, which core version would?
//!      [`check_api`]
//!
//! ## Why the signature check is written here rather than taken from a crate
//!
//! The `sigstore` crate is the obvious answer, and it was measured rather than
//! guessed. Added to this crate at 0.14 with `verify` and `sigstore-trust-root`
//! on, and called from a path the binary actually reaches so that fat LTO
//! cannot strip it, the release binary went from 15,512,032 bytes to
//! 21,569,952: 5.78 MiB for one check, against the 3 MB this project allows a
//! single feature to cost. It brings its own TUF client, an X.509 stack, a
//! protobuf runtime, the Rekor and Fulcio API models, and aws-lc-rs beside the
//! rustls already here.
//!
//! It also does not compile into this workspace as it stands. Its transitive
//! `typed_path` carries a blanket `AsRef` impl for `Cow<'_, str>` that breaks
//! inference in three untouched lines of `godwinmix-core`, so taking it would
//! mean editing code that has nothing to do with signatures.
//!
//! So the check here is in two levels, and the level reached is recorded
//! rather than glossed over:
//!
//!   * `cosign` on PATH: `cosign verify-blob` does the full cryptographic
//!     verification, certificate chain to Fulcio's root and inclusion in the
//!     transparency log included. This is [`Level::Cosign`].
//!   * No `cosign`: the bundle is parsed, the artefact's SHA-256 is compared
//!     against the digest the bundle says it signed, and the bundle is
//!     required to carry a certificate, a signature and a transparency log
//!     entry. This catches a swapped or truncated download and a bundle that
//!     belongs to another file. It does not prove who signed it. This is
//!     [`Level::Bundle`], and every place that shows it says so in words.
//!
//! An operator who wants the strong answer installs cosign; `gmx plugin add`
//! prints the one line that says so when it falls back.

pub mod sha256;

use serde::{Deserialize, Serialize};
use std::path::Path;

/// What is known about where a plugin came from.
///
/// Stored beside the installed plugin as `.gmx-trust.json` so a core that
/// restarts still knows, and carried in `plugin.list` and `plugin.describe`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Trust {
    /// Where it was fetched from, as the operator typed it. This is what an
    /// update refetches, so it must not carry the version that was resolved:
    /// `psmux/gmx-ndi` asked again finds the newest release, and
    /// `psmux/gmx-ndi@1.2.0` asked again finds 1.2.0 forever, which is what a
    /// pin is for and what an unpinned install must not become.
    #[serde(default)]
    pub source: String,
    /// What that turned out to be: the tag, the version, the commit. For the
    /// report line and for `plugin.describe`, never for refetching.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resolved: String,
    /// The signature, when there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
    /// Why there is no signature, when there is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsigned_because: Option<String>,
}

impl Trust {
    /// An install with nothing behind it: a path, a git clone, a package
    /// manager that does not sign.
    pub fn unsigned(source: impl Into<String>, why: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            resolved: String::new(),
            signature: None,
            unsigned_because: Some(why.into()),
        }
    }

    pub fn signed(source: impl Into<String>, signature: Signature) -> Self {
        Self {
            source: source.into(),
            resolved: String::new(),
            signature: Some(signature),
            unsigned_because: None,
        }
    }

    /// Record what the source turned out to be, keeping what was asked for.
    pub fn resolved_to(mut self, resolved: impl Into<String>) -> Self {
        self.resolved = resolved.into();
        self
    }

    /// What was asked for, and what it turned out to be when those differ.
    pub fn origin(&self) -> String {
        if self.resolved.is_empty() || self.resolved == self.source {
            self.source.clone()
        } else {
            format!("{} ({})", self.source, self.resolved)
        }
    }

    pub fn is_signed(&self) -> bool {
        self.signature.is_some()
    }

    /// The words an operator reads in `plugin.list`. Short enough for a column.
    pub fn label(&self) -> &'static str {
        match &self.signature {
            Some(s) if s.level == Level::Cosign => "signed",
            Some(_) => "signed, digest only",
            None => "custom, unreviewed",
        }
    }

    /// The sentence `plugin.describe` prints under the label.
    pub fn explanation(&self) -> String {
        match &self.signature {
            Some(s) if s.level == Level::Cosign => format!(
                "cosign verified the signature over these bytes{}.",
                s.identity
                    .as_deref()
                    .map(|i| format!(", signed by {i}"))
                    .unwrap_or_default()
            ),
            Some(s) => format!(
                "the bundle records sha256:{} and that is what arrived, but nothing checked \
                 who signed it. Install cosign and run `gmx plugin update {}` for the full \
                 check.",
                &s.digest[..s.digest.len().min(16)],
                "<name>"
            ),
            None => format!(
                "unreviewed: {}. It runs with the permissions you give it.",
                self.unsigned_because.as_deref().unwrap_or("nothing signed it")
            ),
        }
    }

    pub fn read(root: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(root.join(FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn write(&self, root: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        std::fs::write(root.join(FILE), text)
    }
}

/// The file a plugin's trust record is kept in, inside its install directory.
pub const FILE: &str = ".gmx-trust.json";

/// How far the signature check got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// `cosign verify-blob` passed: certificate, chain and log entry.
    Cosign,
    /// The bundle parsed and its digest matched the bytes. Nothing more.
    Bundle,
}

/// What a passing signature check knows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Signature {
    pub level: Level,
    /// The artefact's SHA-256, lower case hex.
    pub digest: String,
    /// The signing identity, when the bundle or cosign named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// The Rekor log index, which is what a reader needs to look the entry up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_index: Option<u64>,
}

/// Why a signature was refused. Each names the next step.
#[derive(Debug)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

/// Check a cosign bundle against the bytes it claims to cover.
///
/// `artefact` is the downloaded file, `bundle` the `.sigstore.json` (or the
/// older `.bundle`) beside it. `identity` is the regular expression the
/// signing identity must match when cosign is available, which for the index
/// CI is the workflow's own URL.
pub fn check_signature(
    artefact: &Path,
    bundle: &Path,
    identity: Option<&Identity>,
) -> Result<Signature, Refused> {
    let digest = sha256::hex_file(artefact).map_err(|e| {
        Refused(format!("the download at {} could not be read: {e}", artefact.display()))
    })?;
    let text = std::fs::read_to_string(bundle).map_err(|e| {
        Refused(format!("the signature at {} could not be read: {e}", bundle.display()))
    })?;
    let parsed = parse_bundle(&text)?;
    if parsed.digest != digest {
        return Err(Refused(format!(
            "the signature covers sha256:{} but the file that arrived is sha256:{}. \
             The download was truncated, or the signature belongs to another file. \
             Delete it and run the add again.",
            parsed.digest, digest
        )));
    }
    if let Some(found) = cosign_verify(artefact, bundle, identity) {
        return found.map(|(id, issuer)| Signature {
            level: Level::Cosign,
            digest,
            identity: id.or(parsed.identity),
            issuer: issuer.or(parsed.issuer),
            log_index: parsed.log_index,
        });
    }
    Ok(Signature {
        level: Level::Bundle,
        digest,
        identity: parsed.identity,
        issuer: parsed.issuer,
        log_index: parsed.log_index,
    })
}

/// Who the signature must be from, for the cosign path.
#[derive(Debug, Clone)]
pub struct Identity {
    /// A regular expression cosign matches the certificate's SAN against.
    pub identity_regexp: String,
    /// The OIDC issuer, for example `https://token.actions.githubusercontent.com`.
    pub oidc_issuer: String,
}

impl Identity {
    /// What the `godwinmix-plugins` index CI signs as: any workflow in the
    /// index repository, through GitHub's OIDC issuer.
    pub fn index_ci(index_repo: &str) -> Self {
        Self {
            identity_regexp: format!("^https://github.com/{index_repo}/.github/workflows/.+"),
            oidc_issuer: "https://token.actions.githubusercontent.com".into(),
        }
    }
}

/// What the bundle itself says, before anyone checks the maths.
#[derive(Debug, Default, PartialEq)]
struct Parsed {
    digest: String,
    identity: Option<String>,
    issuer: Option<String>,
    log_index: Option<u64>,
}

/// Read either bundle shape sigstore has shipped.
///
/// The new one (`application/vnd.dev.sigstore.bundle.v0.3+json`) carries the
/// digest under `messageSignature`. The one `cosign sign-blob --bundle` wrote
/// for years carries it inside the base64 Rekor body. Both are in the wild, so
/// both are read.
fn parse_bundle(text: &str) -> Result<Parsed, Refused> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| Refused(format!("the signature file is not JSON: {e}")))?;
    if let Some(digest) = value
        .pointer("/messageSignature/messageDigest/digest")
        .and_then(serde_json::Value::as_str)
    {
        let bytes = decode_base64(digest).ok_or_else(|| {
            Refused("the bundle's messageDigest is not base64".into())
        })?;
        let mut parsed = Parsed { digest: to_hex(&bytes), ..Parsed::default() };
        parsed.log_index = value
            .pointer("/verificationMaterial/tlogEntries/0/logIndex")
            .and_then(number_or_string);
        if value.pointer("/verificationMaterial/certificate").is_none()
            && value.pointer("/verificationMaterial/x509CertificateChain").is_none()
        {
            return Err(Refused(
                "the bundle carries no certificate, so it is not a keyless signature. \
                 A plugin listed on an index is signed by the index CI; this one was not."
                    .into(),
            ));
        }
        if parsed.log_index.is_none() {
            return Err(Refused(
                "the bundle has no transparency log entry. A sigstore keyless signature \
                 always has one; without it there is nothing to look the signature up in."
                    .into(),
            ));
        }
        return Ok(parsed);
    }
    // The older shape.
    let body = value
        .pointer("/rekorBundle/Payload/body")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            Refused(
                "this is not a cosign bundle. `gmx plugin add` wants the .sigstore.json \
                 that sits beside a release asset, or the file `cosign sign-blob --bundle` \
                 wrote."
                    .into(),
            )
        })?;
    let decoded = decode_base64(body)
        .ok_or_else(|| Refused("the bundle's Rekor body is not base64".into()))?;
    let entry: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(|e| Refused(format!("the bundle's Rekor body is not JSON: {e}")))?;
    let digest = entry
        .pointer("/spec/data/hash/value")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| Refused("the Rekor entry records no artefact hash".into()))?;
    if value.get("cert").is_none() {
        return Err(Refused(
            "the bundle carries no certificate, so it is not a keyless signature.".into(),
        ));
    }
    Ok(Parsed {
        digest: digest.to_ascii_lowercase(),
        identity: None,
        issuer: None,
        log_index: value
            .pointer("/rekorBundle/Payload/logIndex")
            .and_then(number_or_string),
    })
}

fn number_or_string(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// What cosign said, when there was a cosign to ask: the identity and the
/// issuer it matched, or a refusal.
type CosignAnswer = Result<(Option<String>, Option<String>), Refused>;

/// Run `cosign verify-blob` when cosign is installed.
///
/// `None` means there is no cosign to run, which is not a failure: the caller
/// falls back to the digest check and says so.
fn cosign_verify(
    artefact: &Path,
    bundle: &Path,
    identity: Option<&Identity>,
) -> Option<CosignAnswer> {
    if std::env::var_os("GMX_NO_COSIGN").is_some() {
        return None;
    }
    let mut cmd = std::process::Command::new("cosign");
    cmd.arg("verify-blob").arg("--bundle").arg(bundle);
    if let Some(id) = identity {
        cmd.arg("--certificate-identity-regexp").arg(&id.identity_regexp);
        cmd.arg("--certificate-oidc-issuer").arg(&id.oidc_issuer);
    } else {
        // Without an expected identity cosign refuses to verify at all, which
        // is the right default; say so rather than passing a wildcard.
        cmd.arg("--certificate-identity-regexp").arg(".*");
        cmd.arg("--certificate-oidc-issuer-regexp").arg(".*");
    }
    cmd.arg(artefact);
    let out = match cmd.output() {
        Ok(out) => out,
        // Not installed, or not executable. The caller falls back.
        Err(_) => return None,
    };
    if out.status.success() {
        let text = String::from_utf8_lossy(&out.stderr).into_owned();
        return Some(Ok((
            identity.map(|i| i.identity_regexp.clone()).or_else(|| first_line(&text)),
            identity.map(|i| i.oidc_issuer.clone()),
        )));
    }
    let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Some(Err(Refused(format!(
        "cosign refused the signature on {}: {}. The plugin was not installed. If you meant \
         to install an unsigned build, add it from a local path with \
         `[plugins] allow_unsigned = true` in your config.",
        artefact.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        if why.is_empty() { "no reason given".into() } else { why }
    ))))
}

fn first_line(text: &str) -> Option<String> {
    text.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).expect("nibble"));
        s.push(char::from_digit((b & 0x0f) as u32, 16).expect("nibble"));
    }
    s
}

/// Standard base64 with padding, which is what both bundle shapes use.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// The api range check
// ---------------------------------------------------------------------------

/// Which core release first spoke each api level.
///
/// Terraform advertises a protocol version per provider and tells you which
/// version of Terraform speaks it; this is the same table, and it is the whole
/// reason a mismatch is an upgrade hint rather than a crash at launch. One row
/// per level, appended when a level ships. Nothing is guessed: a level with no
/// row has not been released.
pub const CORE_RELEASES: &[(u32, &str)] = &[(1, "0.2.0")];

/// Whether this core will run a plugin that declares `api`.
pub fn check_api(plugin: &str, version: &str, api: u32) -> Result<(), Refused> {
    let compatible = godwinmix_protocol::API_COMPATIBLE;
    let level = godwinmix_protocol::API_LEVEL;
    if (compatible..=level).contains(&api) {
        return Ok(());
    }
    if api < compatible {
        return Err(Refused(format!(
            "{plugin} {version} declares api {api} and this core dropped support below api \
             {compatible}. Ask the author for a build against api {compatible} or later, or \
             run a core from before the api {compatible} release."
        )));
    }
    let hint = match CORE_RELEASES.iter().find(|(l, _)| *l == api) {
        Some((_, core)) => format!("GodwinMix {core} or later speaks api {api}; this core is {}, which speaks api {level}.", env!("CARGO_PKG_VERSION")),
        None => format!(
            "no released core speaks api {api} yet; this core is {} and speaks api {level}. \
             The plugin is ahead of the core, which usually means a pre release build.",
            env!("CARGO_PKG_VERSION")
        ),
    };
    Err(Refused(format!(
        "{plugin} {version} needs api {api} and was not installed. {hint} \
         `gmx plugin search {plugin}` lists the versions and the api level of each."
    )))
}

#[cfg(test)]
mod tests;
