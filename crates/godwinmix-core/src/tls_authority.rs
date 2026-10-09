//! A small certificate authority for the control port, one per machine.
//!
//! A phone will only let a person trust a certificate authority: iOS lists
//! nothing else under Certificate Trust Settings, and Android's "CA
//! certificate" installer wants one too. So the control port's certificate is
//! two levels. The authority is made once, lives ten years, and is what a
//! person installs on a phone. The certificate the port serves is signed by
//! it, covers the names the machine is reached by, and is made again when an
//! address changes or it nears its end, without the phone noticing, because
//! the phone trusts the authority and not the certificate.
//!
//! The authority signs nothing but server certificates for this machine, and
//! its Basic Constraints say so with a path length of zero.

use anyhow::{anyhow, Context, Result};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, DnValue, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;

use crate::tls_cert::{civil, days_since_epoch, Pair, SERVER_DAYS};

/// How long the authority lasts. Ten years, because a new one means every
/// phone that trusts this mixer has to be set up again.
pub const AUTHORITY_DAYS: i64 = 3650;

/// The authority's name as a person sees it in a phone's settings.
pub fn authority_name(host: Option<&str>) -> String {
    match host {
        Some(host) => format!("GodwinMix local authority on {host}"),
        None => "GodwinMix local authority".to_string(),
    }
}

/// A new authority called `name`: its certificate and its key, as PEM.
pub fn make_authority(name: &str) -> Result<Pair> {
    let key = KeyPair::generate().context("making the authority's key")?;
    let mut params = authority_params(name);
    validity(&mut params, AUTHORITY_DAYS);
    let cert = params.self_signed(&key).context("signing the authority's certificate")?;
    Ok(Pair { cert: cert.pem(), key: key.serialize_pem() })
}

/// What the authority's certificate says about itself. Built the same way
/// when signing, because `signed_by` reads the issuer's name and key
/// identifier off a certificate object, and rebuilding one from these
/// parameters and the kept key is cheaper than an X.509 parser.
fn authority_params(name: &str) -> CertificateParams {
    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.distinguished_name.push(DnType::CommonName, DnValue::Utf8String(name.into()));
    params.distinguished_name.push(DnType::OrganizationName, DnValue::Utf8String("GodwinMix".into()));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign, KeyUsagePurpose::DigitalSignature];
    params
}

fn validity(params: &mut CertificateParams, days: i64) {
    let today = days_since_epoch();
    let (y, m, d) = civil(today - 1);
    params.not_before = rcgen::date_time_ymd(y, m, d);
    let (y, m, d) = civil(today + days);
    params.not_after = rcgen::date_time_ymd(y, m, d);
}

/// A certificate for a web server reached at `names`, signed by the
/// authority `ca` called `ca_name`, with `label` as its common name. The PEM
/// it returns is the chain: the server's certificate, then the authority's.
pub fn issue_server(ca: &Pair, ca_name: &str, names: &[String], label: &str) -> Result<Pair> {
    let ca_key = KeyPair::from_pem(&ca.key).map_err(|e| anyhow!("the authority's key cannot be read ({e})"))?;
    let issuer = rebuilt(ca, &ca_key, ca_name)?;
    let mut params = CertificateParams::new(names.to_vec()).context("those names cannot go in a certificate")?;
    params.distinguished_name.push(DnType::CommonName, DnValue::Utf8String(label.into()));
    params.is_ca = IsCa::ExplicitNoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    validity(&mut params, SERVER_DAYS);
    let key = KeyPair::generate().context("making a key for the control port")?;
    let cert = params.signed_by(&key, &issuer, &ca_key).context("signing the control port's certificate")?;
    Ok(Pair { cert: format!("{}{}", cert.pem(), ca.cert), key: key.serialize_pem() })
}

/// The authority as an issuer, after checking the kept key is the one its
/// certificate was made with.
fn rebuilt(ca: &Pair, key: &KeyPair, name: &str) -> Result<Certificate> {
    let der = CertificateDer::from_pem_slice(ca.cert.as_bytes()).map_err(|e| anyhow!("the authority's certificate cannot be read ({e})"))?;
    let public = key.public_key_raw();
    if !der.as_ref().windows(public.len()).any(|w| w == public) {
        return Err(anyhow!("the authority's key does not belong to its certificate"));
    }
    authority_params(name).self_signed(key).context("rebuilding the authority to sign with")
}

#[cfg(test)]
#[path = "tls_authority_tests.rs"]
mod tests;
