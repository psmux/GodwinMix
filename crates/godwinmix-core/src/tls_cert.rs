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

/// How long a certificate made by [`for_server`] lasts. Under the 825 days
/// Apple's platforms accept for a server certificate a person has trusted by
/// hand, so importing it into the keychain works.
pub const SERVER_DAYS: i64 = 800;

/// A self signed certificate for a web server reached at `names`, with what
/// a browser looks for when a person trusts it by hand: the names as subject
/// alternative names, the server authentication purpose, a lifetime under
/// [`SERVER_DAYS`], and `label` as the common name a certificate viewer shows.
pub fn for_server(names: &[String], label: &str) -> Result<Pair> {
    let mut params = rcgen::CertificateParams::new(names.to_vec()).context("those names cannot go in a certificate")?;
    params.distinguished_name.push(rcgen::DnType::CommonName, label);
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let today = days_since_epoch();
    let (y, m, d) = civil(today - 1);
    params.not_before = rcgen::date_time_ymd(y, m, d);
    let (y, m, d) = civil(today + SERVER_DAYS);
    params.not_after = rcgen::date_time_ymd(y, m, d);
    let key = rcgen::KeyPair::generate().context("making a key pair")?;
    let cert = params.self_signed(&key).context("signing the certificate")?;
    Ok(Pair { cert: cert.pem(), key: key.serialize_pem() })
}

/// Days since 1970-01-01, today.
pub fn days_since_epoch() -> i64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (secs / 86_400) as i64
}

/// The calendar date `days` after 1970-01-01, by Howard Hinnant's
/// `civil_from_days`, so this needs no date crate.
pub(crate) fn civil(days: i64) -> (i32, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    (year, month, day)
}

/// A rustls server configuration for this pair. The error says which half is
/// wrong in words a person uploading them can act on.
pub fn server_config(pair: &Pair) -> Result<rustls::ServerConfig> {
    let certs = certificates(&pair.cert)?;
    let key = PrivateKeyDer::from_pem_slice(pair.key.as_bytes())
        .map_err(|e| anyhow!("the private key is not PEM this can read ({e}). Upload the .key file that came with the certificate."))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| anyhow!("the certificate and the key do not go together ({e}). Upload the key that belongs to this certificate."))
}

/// Would a TLS server start with this pair?
pub fn check(pair: &Pair) -> Result<()> {
    server_config(pair).map(|_| ())
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
    fn a_server_certificate_names_addresses_and_checks() {
        let names = vec!["localhost".to_string(), "192.168.1.20".to_string(), "::1".to_string()];
        let pair = for_server(&names, "GodwinMix test").unwrap();
        check(&pair).expect("its own key goes with it");
        assert_ne!(fingerprint(&pair.cert).unwrap(), fingerprint(&for_server(&names, "x").unwrap().cert).unwrap());
    }

    #[test]
    fn days_become_the_calendar_dates_they_are() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(civil(20_727), (2026, 10, 1));
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
