//! The core's own certificate authority.
//!
//! A node and the core speak mutual TLS. Nobody wants to run a PKI to put a
//! second machine in a church hall, so the core is the authority: it makes a
//! key and a self signed root on first run, keeps them under its runtime
//! directory at mode 0600, and signs one certificate per node with a SPIFFE
//! style identity (`spiffe://godwinmix/node/studio-b`). Both sides trust that
//! root and nothing else, so a certificate from a public CA does not get a
//! node in, and neither does a certificate this core did not sign.
//!
//! Rotation is by reissue: `gmx node token --name studio-b` mints a fresh
//! enrolment token, the node enrols again, and the new certificate replaces
//! the old one on disk. Revocation is `node.remove`, which forgets the node;
//! the next connection presenting that identity is refused because the name is
//! no longer known. There is no CRL and there does not need to be, because the
//! core is the only relying party.

use anyhow::{Context, Result};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, DnValue, Ia5String, IsCa, KeyPair, KeyUsagePurpose,
    SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The trust domain every identity this core issues lives in.
pub const TRUST_DOMAIN: &str = "godwinmix";

/// How long an issued node certificate is good for. A year, so a fixed
/// installation is not a maintenance job, and short enough that a machine that
/// left the building stops working eventually.
pub const NODE_CERT_DAYS: u64 = 365;

/// The SPIFFE style identity of one node.
pub fn spiffe_of(name: &str) -> String {
    format!("spiffe://{TRUST_DOMAIN}/node/{name}")
}

/// The node name inside a SPIFFE URI, if it is one of ours.
pub fn name_in_spiffe(uri: &str) -> Option<&str> {
    uri.strip_prefix(&format!("spiffe://{TRUST_DOMAIN}/node/"))
        .filter(|n| !n.is_empty())
}

/// A key and certificate, in PEM, as they are handed to a node.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Issued {
    pub cert_pem: String,
    pub key_pem: String,
    /// The root the other side must trust.
    pub ca_pem: String,
    /// The identity in the certificate, for the log line and for `node.get`.
    pub identity: String,
}

/// The core's authority: a root key, its certificate, and the directory both
/// live in.
pub struct NodeCa {
    dir: PathBuf,
    key: KeyPair,
    ca_pem: String,
    ca_der: CertificateDer<'static>,
}

impl NodeCa {
    /// Load the authority from `dir`, making it on first run.
    ///
    /// `dir` is `<runtime>/ca`. Two files: `ca.key.pem` and `ca.pem`.
    pub fn open_or_create(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("make the CA directory {}", dir.display()))?;
        let key_path = dir.join("ca.key.pem");
        let cert_path = dir.join("ca.pem");
        if key_path.exists() && cert_path.exists() {
            let key_pem = std::fs::read_to_string(&key_path)
                .with_context(|| format!("read {}", key_path.display()))?;
            let key = KeyPair::from_pem(&key_pem).context(
                "the CA key under the runtime directory did not parse. Move ca.key.pem and \
                 ca.pem aside and every node will have to enrol again",
            )?;
            let ca_pem = std::fs::read_to_string(&cert_path)
                .with_context(|| format!("read {}", cert_path.display()))?;
            let ca_der = first_cert(&ca_pem)?;
            return Ok(Self { dir: dir.to_path_buf(), key, ca_pem, ca_der });
        }
        let key = KeyPair::generate().context("generate the CA key")?;
        let cert = root_params()?.self_signed(&key).context("sign the CA certificate")?;
        let ca_pem = cert.pem();
        write_private(&key_path, &key.serialize_pem())?;
        std::fs::write(&cert_path, &ca_pem)
            .with_context(|| format!("write {}", cert_path.display()))?;
        let ca_der = first_cert(&ca_pem)?;
        tracing::info!(dir = %dir.display(), "made the node certificate authority");
        Ok(Self { dir: dir.to_path_buf(), key, ca_pem, ca_der })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn ca_pem(&self) -> &str {
        &self.ca_pem
    }

    /// Sign a certificate for one node.
    pub fn issue_node(&self, name: &str) -> Result<Issued> {
        let identity = spiffe_of(name);
        let mut params = CertificateParams::default();
        params
            .distinguished_name
            .push(DnType::CommonName, DnValue::Utf8String(name.to_string()));
        params.subject_alt_names = vec![
            SanType::URI(Ia5String::try_from(identity.clone()).context("the node name is not ASCII")?),
            SanType::DnsName(
                Ia5String::try_from(name.to_string())
                    .context("the node name is not a legal DNS label")?,
            ),
        ];
        params.use_authority_key_identifier_extension = true;
        params.key_usages =
            vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
        params.extended_key_usages = vec![
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
        ];
        params.not_after = days_from_now(NODE_CERT_DAYS);
        self.sign(params, identity)
    }

    /// Sign the core's own server certificate, covering every name a node might
    /// dial it by.
    pub fn issue_server(&self, names: &[String]) -> Result<Issued> {
        let identity = format!("spiffe://{TRUST_DOMAIN}/core");
        let mut params = CertificateParams::default();
        params
            .distinguished_name
            .push(DnType::CommonName, DnValue::Utf8String("godwinmix core".into()));
        let mut sans = vec![SanType::URI(
            Ia5String::try_from(identity.clone()).expect("the core identity is ASCII"),
        )];
        for n in names {
            match n.parse::<std::net::IpAddr>() {
                Ok(ip) => sans.push(SanType::IpAddress(ip)),
                Err(_) => {
                    if let Ok(dns) = Ia5String::try_from(n.clone()) {
                        sans.push(SanType::DnsName(dns));
                    }
                }
            }
        }
        params.subject_alt_names = sans;
        params.use_authority_key_identifier_extension = true;
        params.key_usages =
            vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        params.not_after = days_from_now(NODE_CERT_DAYS);
        self.sign(params, identity)
    }

    fn sign(&self, params: CertificateParams, identity: String) -> Result<Issued> {
        let leaf_key = KeyPair::generate().context("generate a leaf key")?;
        // `signed_by` reads only the issuer's distinguished name, its key
        // identifier method and its key usages off this object, so rebuilding
        // it from the same parameters and the same key gives a certificate
        // that verifies against the root PEM on disk. It is cheaper than
        // dragging in an X.509 parser to read back what we wrote.
        let issuer = root_params()?.self_signed(&self.key).context("rebuild the CA issuer")?;
        let cert = params
            .signed_by(&leaf_key, &issuer, &self.key)
            .context("sign the leaf certificate")?;
        Ok(Issued {
            cert_pem: cert.pem(),
            key_pem: leaf_key.serialize_pem(),
            ca_pem: self.ca_pem.clone(),
            identity,
        })
    }

    /// The core's own server certificate, made once and kept.
    pub fn server_identity(&self, names: &[String]) -> Result<Issued> {
        let path = self.dir.join("core.json");
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(found) = serde_json::from_str::<Issued>(&text) {
                if found.ca_pem == self.ca_pem {
                    return Ok(found);
                }
            }
        }
        let issued = self.issue_server(names)?;
        write_private(&path, &serde_json::to_string(&issued)?)?;
        Ok(issued)
    }

    /// A TLS server that asks for a client certificate but does not demand
    /// one, because an enrolling node has none yet. The bridge checks for the
    /// certificate itself and refuses the connection when it is missing.
    pub fn server_config(&self, names: &[String]) -> Result<Arc<ServerConfig>> {
        let me = self.server_identity(names)?;
        let provider = provider();
        let mut roots = RootCertStore::empty();
        roots.add(self.ca_der.clone()).context("trust our own root")?;
        let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
            Arc::new(roots),
            provider.clone(),
        )
        .allow_unauthenticated()
        .build()
        .context("build the client certificate verifier")?;
        let cfg = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .context("pick TLS versions")?
            .with_client_cert_verifier(verifier)
            .with_single_cert(vec![first_cert(&me.cert_pem)?], private_key(&me.key_pem)?)
            .context("load the core's own certificate")?;
        Ok(Arc::new(cfg))
    }
}

/// A TLS client that trusts one root and presents one identity. `identity` is
/// `None` while enrolling, when the node has nothing to present yet.
pub fn client_config(ca_pem: &str, identity: Option<&Issued>) -> Result<Arc<ClientConfig>> {
    let provider = provider();
    let mut roots = RootCertStore::empty();
    roots.add(first_cert(ca_pem)?).context("trust the core's root")?;
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("pick TLS versions")?
        .with_root_certificates(roots);
    let cfg = match identity {
        Some(me) => builder
            .with_client_auth_cert(vec![first_cert(&me.cert_pem)?], private_key(&me.key_pem)?)
            .context("load this node's certificate")?,
        None => builder.with_no_client_auth(),
    };
    Ok(Arc::new(cfg))
}

/// The node name in a presented client certificate, read out of its SPIFFE
/// URI subject alternative name.
///
/// The SAN is a DER `[6] IA5String`, which is the bytes `86 <len> <ascii>`, so
/// the identity is plain ASCII inside the certificate and finding it is a
/// scan rather than a parse. That is enough here because the only certificates
/// this ever sees are ones this core signed: anything else was already refused
/// by the TLS layer before the bytes arrive.
pub fn identity_in(cert: &CertificateDer<'_>) -> Option<String> {
    let prefix = format!("spiffe://{TRUST_DOMAIN}/node/").into_bytes();
    let der = cert.as_ref();
    for start in 0..der.len().saturating_sub(prefix.len()) {
        if der[start..].starts_with(&prefix) {
            // Walk back two bytes for the tag and the length, and trust the
            // length rather than guessing where the name ends.
            if start < 2 || der[start - 2] != 0x86 {
                continue;
            }
            let len = der[start - 1] as usize;
            let end = start + len;
            if len < prefix.len() || end > der.len() {
                continue;
            }
            return std::str::from_utf8(&der[start..end]).ok().map(|s| s.to_string());
        }
    }
    None
}

fn root_params() -> Result<CertificateParams> {
    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(1));
    params
        .distinguished_name
        .push(DnType::CommonName, DnValue::Utf8String("godwinmix node CA".into()));
    params
        .distinguished_name
        .push(DnType::OrganizationName, DnValue::Utf8String(TRUST_DOMAIN.into()));
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    // Ten years. The root outlives the machine it is on, and reissuing it
    // means every node enrols again, which is the one thing an operator should
    // not have to do on a schedule.
    params.not_after = days_from_now(3650);
    Ok(params)
}

/// A calendar date `days` from today, which is the only shape rcgen takes.
///
/// Doing the civil calendar here rather than taking the `time` crate as a
/// direct dependency: this is the only date arithmetic in the engine and it is
/// fifteen lines of Howard Hinnant's `civil_from_days`.
fn days_from_now(days: u64) -> time::OffsetDateTime {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((now / 86_400 + days) as i64);
    rcgen::date_time_ymd(y, m, d)
}

/// Days since 1970-01-01 to a year, month and day.
fn civil_from_days(z: i64) -> (i32, u8, u8) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    ((y + i64::from(m <= 2)) as i32, m as u8, d as u8)
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn first_cert(pem: &str) -> Result<CertificateDer<'static>> {
    let der = pem_body(pem, "CERTIFICATE").context("no CERTIFICATE block in this PEM")?;
    Ok(CertificateDer::from(der))
}

fn private_key(pem: &str) -> Result<PrivateKeyDer<'static>> {
    let der = pem_body(pem, "PRIVATE KEY").context("no PRIVATE KEY block in this PEM")?;
    Ok(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(der)))
}

/// The base64 body of the first PEM block whose label ends with `label`.
fn pem_body(pem: &str, label: &str) -> Option<Vec<u8>> {
    let mut body = String::new();
    let mut inside = false;
    for line in pem.lines() {
        let line = line.trim();
        if line.starts_with("-----BEGIN ") && line.trim_end_matches('-').ends_with(label) {
            inside = true;
            continue;
        }
        if line.starts_with("-----END ") {
            if inside {
                break;
            }
            continue;
        }
        if inside {
            body.push_str(line);
        }
    }
    if body.is_empty() {
        return None;
    }
    base64_decode(&body)
}

/// Standard base64 with padding, decoded by hand. The core has no base64 crate
/// and one PEM body is not worth adding one.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Write a file only the owner can read. On Windows the ACL the user's profile
/// directory carries is the protection; there is no mode to set.
fn write_private(path: &Path, contents: &str) -> Result<()> {
    std::fs::write(path, contents).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restrict {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_certificate_carries_its_spiffe_identity() {
        let dir = std::env::temp_dir().join(format!("gmx-ca-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ca = NodeCa::open_or_create(&dir).unwrap();
        let issued = ca.issue_node("studio-b").unwrap();
        assert_eq!(issued.identity, "spiffe://godwinmix/node/studio-b");
        let der = first_cert(&issued.cert_pem).unwrap();
        assert_eq!(identity_in(&der).as_deref(), Some("spiffe://godwinmix/node/studio-b"));
        assert_eq!(name_in_spiffe(&issued.identity), Some("studio-b"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_authority_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("gmx-ca2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = NodeCa::open_or_create(&dir).unwrap().ca_pem().to_string();
        let again = NodeCa::open_or_create(&dir).unwrap().ca_pem().to_string();
        assert_eq!(first, again, "a second open must not mint a second root");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn both_sides_can_build_a_tls_config() {
        let dir = std::env::temp_dir().join(format!("gmx-ca3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ca = NodeCa::open_or_create(&dir).unwrap();
        ca.server_config(&["localhost".into(), "127.0.0.1".into()]).unwrap();
        let node = ca.issue_node("studio-b").unwrap();
        client_config(ca.ca_pem(), Some(&node)).unwrap();
        client_config(ca.ca_pem(), None).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
