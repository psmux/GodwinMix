use super::*;
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::{ServerName, UnixTime};
use std::sync::Arc;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn ders(pem: &str) -> Vec<CertificateDer<'static>> {
    CertificateDer::pem_slice_iter(pem.as_bytes()).collect::<Result<_, _>>().unwrap()
}

fn has(der: &[u8], bytes: &[u8]) -> bool {
    der.windows(bytes.len()).any(|w| w == bytes)
}

/// Would a client that trusts `ca` accept `chain` for `name`?
fn trusted(ca: &Pair, chain: &str, name: &str) -> bool {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(ders(&ca.cert).remove(0)).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider).build().unwrap();
    let certs = ders(chain);
    let name = ServerName::try_from(name.to_string()).unwrap();
    verifier.verify_server_cert(&certs[0], &certs[1..], &name, &[], UnixTime::now()).is_ok()
}

#[test]
fn the_authority_says_it_is_one_and_may_only_sign_certificates_for_servers() {
    let ca = make_authority(&authority_name(Some("studio-pc"))).unwrap();
    let der = ders(&ca.cert).remove(0);
    // Basic Constraints: cA TRUE, pathLenConstraint 0.
    assert!(has(der.as_ref(), &[0x30, 0x06, 0x01, 0x01, 0xFF, 0x02, 0x01, 0x00]), "CA:TRUE, pathlen:0");
    // Key Usage: digitalSignature, keyCertSign, cRLSign.
    // The bit string openssl prints as "Digital Signature, Certificate Sign, CRL Sign".
    assert!(has(der.as_ref(), &[0x55, 0x1D, 0x0F]), "a key usage extension");
    assert!(has(der.as_ref(), &[0x03, 0x03, 0x07, 0x86, 0x00]), "keyCertSign among the usages");
    assert!(has(der.as_ref(), b"GodwinMix local authority on studio-pc"));
}

#[test]
fn the_server_certificate_is_signed_by_the_authority_for_every_name_it_covers() {
    let name = authority_name(Some("studio-pc"));
    let ca = make_authority(&name).unwrap();
    let wanted = names(&["localhost", "127.0.0.1", "::1", "studio-pc.local", "192.168.1.20"]);
    let server = issue_server(&ca, &name, &wanted, "GodwinMix on studio-pc").unwrap();
    crate::tls_cert::check(&server).expect("the chain and its key make a TLS server");
    assert_eq!(ders(&server.cert).len(), 2, "the server's certificate, then the authority's");
    for n in &wanted {
        assert!(trusted(&ca, &server.cert, n), "{n} is not covered");
    }
    assert!(!trusted(&ca, &server.cert, "elsewhere.example"), "a name it was not made for");
    let leaf = ders(&server.cert).remove(0);
    let ca_true = [0x01, 0x01, 0xFF, 0x02, 0x01, 0x00];
    assert!(!has(leaf.as_ref(), &ca_true), "the server's certificate is not an authority");
}

#[test]
fn another_authority_does_not_vouch_for_it_and_a_wrong_key_is_refused() {
    let name = authority_name(None);
    let ca = make_authority(&name).unwrap();
    let other = make_authority(&name).unwrap();
    let server = issue_server(&ca, &name, &names(&["localhost"]), "x").unwrap();
    assert!(!trusted(&other, &server.cert, "localhost"));
    let crossed = Pair { cert: ca.cert.clone(), key: other.key.clone() };
    let why = issue_server(&crossed, &name, &names(&["localhost"]), "x").unwrap_err().to_string();
    assert!(why.contains("does not belong"), "{why}");
}

#[test]
fn a_second_server_certificate_from_the_same_authority_is_trusted_the_same_way() {
    let name = authority_name(Some("studio-pc"));
    let ca = make_authority(&name).unwrap();
    let first = issue_server(&ca, &name, &names(&["10.0.0.5"]), "x").unwrap();
    let moved = issue_server(&ca, &name, &names(&["10.0.0.9"]), "x").unwrap();
    assert_ne!(first.cert, moved.cert);
    assert!(trusted(&ca, &moved.cert, "10.0.0.9"), "a phone that trusted the authority trusts the new one");
}
