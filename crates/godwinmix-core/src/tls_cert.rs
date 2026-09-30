//! A server certificate for RTMPS: one a person uploads, checked, or one made
//! here, self signed.
//!
//! The same crates the node certificate authority uses (`rcgen` to make one,
//! rustls to check one), so this adds nothing to the build.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// A certificate and its private key, as PEM.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub cert: String,
    pub key: String,
}

/// A self signed certificate for `names`: host names, or addresses.
pub fn self_signed(names: &[String]) -> Result<Pair> {
    let made = rcgen::generate_simple_self_signed(names.to_vec()).context("making a self signed certificate")?;
    Ok(Pair { cert: made.cert.pem(), key: made.key_pair.serialize_pem() })
}

/// Would a TLS server start with this pair? The error says which half is
/// wrong in words a person uploading them can act on.
pub fn check(pair: &Pair) -> Result<()> {
    let certs = certificates(&pair.cert)?;
    let key = PrivateKeyDer::from_pem_slice(pair.key.as_bytes())
        .map_err(|e| anyhow!("the private key is not PEM this can read ({e}). Upload the .key file that came with the certificate."))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| anyhow!("the certificate and the key do not go together ({e}). Upload the key that belongs to this certificate."))?;
    Ok(())
}

/// SHA-256 of the first certificate, as colon separated hex.
pub fn fingerprint(cert_pem: &str) -> Result<String> {
    let certs = certificates(cert_pem)?;
    let digest = ring::digest::digest(&ring::digest::SHA256, certs[0].as_ref());
    Ok(digest.as_ref().iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":"))
}

fn certificates(pem: &str) -> Result<Vec<CertificateDer<'static>>> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(pem.as_bytes())
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow!("the certificate is not PEM this can read ({e}). Upload the .crt or .pem file, the one that begins BEGIN CERTIFICATE."))?;
    if certs.is_empty() {
        return Err(anyhow!("that file has no certificate in it. Upload the .crt or .pem file, the one that begins BEGIN CERTIFICATE."));
    }
    Ok(certs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_self_signed_pair_checks_and_has_a_fingerprint() {
        let pair = self_signed(&["localhost".into(), "127.0.0.1".into()]).unwrap();
        check(&pair).expect("its own key goes with it");
        let print = fingerprint(&pair.cert).unwrap();
        assert_eq!(print.len(), 32 * 3 - 1);
    }

    #[test]
    fn a_key_from_another_pair_or_no_certificate_is_refused_with_what_to_upload() {
        let one = self_signed(&["a.example".into()]).unwrap();
        let two = self_signed(&["b.example".into()]).unwrap();
        let crossed = Pair { cert: one.cert.clone(), key: two.key };
        let why = check(&crossed).unwrap_err().to_string();
        assert!(why.contains("do not go together"), "{why}");
        let empty = check(&Pair { cert: "hello".into(), key: one.key }).unwrap_err().to_string();
        assert!(empty.contains("BEGIN CERTIFICATE"), "{empty}");
    }
}
