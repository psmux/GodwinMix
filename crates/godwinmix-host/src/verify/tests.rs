use super::*;

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("gmx-verify-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// A bundle in the current sigstore shape, covering `bytes`.
fn bundle_for(bytes: &[u8]) -> String {
    let digest = sha256::digest(bytes);
    serde_json::json!({
        "mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json",
        "verificationMaterial": {
            "certificate": { "rawBytes": "MIIB" },
            "tlogEntries": [{ "logIndex": "184729", "integratedTime": "1757800000" }]
        },
        "messageSignature": {
            "messageDigest": { "algorithm": "SHA2_256", "digest": base64_of(&digest) },
            "signature": "MEUCIQ"
        }
    })
    .to_string()
}

fn base64_of(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - i * 6)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn base64_round_trips_through_the_decoder() {
    let cases: Vec<Vec<u8>> =
        vec![vec![], vec![1], vec![1, 2], vec![1, 2, 3], (0u8..=255).collect()];
    for case in cases {
        assert_eq!(decode_base64(&base64_of(&case)), Some(case));
    }
}

#[test]
fn a_bundle_whose_digest_matches_the_file_passes_at_the_bundle_level() {
    std::env::set_var("GMX_NO_COSIGN", "1");
    let dir = temp("match");
    let asset = dir.join("gmx-clock-linux-x86_64.tar.gz");
    std::fs::write(&asset, b"pretend this is a tarball").expect("the asset");
    let sig = dir.join("gmx-clock-linux-x86_64.tar.gz.sigstore.json");
    std::fs::write(&sig, bundle_for(b"pretend this is a tarball")).expect("the bundle");

    let found = check_signature(&asset, &sig, None).expect("it verifies");
    assert_eq!(found.level, Level::Bundle);
    assert_eq!(found.digest, sha256::hex(b"pretend this is a tarball"));
    assert_eq!(found.log_index, Some(184_729));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bundle_for_another_file_is_refused_and_says_what_to_do() {
    std::env::set_var("GMX_NO_COSIGN", "1");
    let dir = temp("mismatch");
    let asset = dir.join("asset.tar.gz");
    std::fs::write(&asset, b"the bytes that arrived").expect("the asset");
    let sig = dir.join("asset.tar.gz.sigstore.json");
    std::fs::write(&sig, bundle_for(b"the bytes that were signed")).expect("the bundle");

    let err = check_signature(&asset, &sig, None).expect_err("the digests differ");
    let text = format!("{err}");
    assert!(text.contains("truncated"), "{text}");
    assert!(text.contains("run the add again"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bundle_with_no_certificate_is_not_a_keyless_signature() {
    std::env::set_var("GMX_NO_COSIGN", "1");
    let dir = temp("nocert");
    let asset = dir.join("asset.tar.gz");
    std::fs::write(&asset, b"bytes").expect("the asset");
    let sig = dir.join("asset.tar.gz.sigstore.json");
    let stripped = serde_json::json!({
        "verificationMaterial": { "tlogEntries": [{ "logIndex": "1" }] },
        "messageSignature": {
            "messageDigest": { "digest": base64_of(&sha256::digest(b"bytes")) }
        }
    });
    std::fs::write(&sig, stripped.to_string()).expect("the bundle");
    let err = check_signature(&asset, &sig, None).expect_err("no certificate");
    assert!(format!("{err}").contains("no certificate"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_older_cosign_bundle_shape_is_read_too() {
    std::env::set_var("GMX_NO_COSIGN", "1");
    let dir = temp("legacy");
    let asset = dir.join("asset.tar.gz");
    std::fs::write(&asset, b"legacy bytes").expect("the asset");
    let body = serde_json::json!({
        "apiVersion": "0.0.1",
        "kind": "hashedrekord",
        "spec": { "data": { "hash": { "algorithm": "sha256", "value": sha256::hex(b"legacy bytes") } } }
    })
    .to_string();
    let bundle = serde_json::json!({
        "base64Signature": "MEUCIQ",
        "cert": "LS0tLS1CRUdJTg==",
        "rekorBundle": { "Payload": { "body": base64_of(body.as_bytes()), "logIndex": 99 } }
    });
    let sig = dir.join("asset.tar.gz.bundle");
    std::fs::write(&sig, bundle.to_string()).expect("the bundle");
    let found = check_signature(&asset, &sig, None).expect("it verifies");
    assert_eq!(found.digest, sha256::hex(b"legacy bytes"));
    assert_eq!(found.log_index, Some(99));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn something_that_is_not_a_bundle_at_all_says_what_was_wanted() {
    std::env::set_var("GMX_NO_COSIGN", "1");
    let dir = temp("garbage");
    let asset = dir.join("asset.tar.gz");
    std::fs::write(&asset, b"bytes").expect("the asset");
    let sig = dir.join("asset.tar.gz.sigstore.json");
    std::fs::write(&sig, "{\"hello\":\"world\"}").expect("not a bundle");
    let err = check_signature(&asset, &sig, None).expect_err("not a bundle");
    assert!(format!("{err}").contains(".sigstore.json"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_api_level_this_core_speaks_is_accepted() {
    check_api("ndi", "1.0.0", godwinmix_protocol::API_LEVEL).expect("this core speaks its own level");
}

#[test]
fn an_api_level_ahead_of_the_core_names_the_core_that_would_run_it() {
    let err = check_api("ndi", "2.0.0", 7).expect_err("api 7 does not exist");
    let text = format!("{err}");
    assert!(text.contains("no released core speaks api 7"), "{text}");
    assert!(text.contains("gmx plugin search ndi"), "{text}");
}

#[test]
fn the_labels_say_what_was_and_was_not_checked() {
    let unsigned = Trust::unsigned("./my-plugin", "it was installed from a local path");
    assert_eq!(unsigned.label(), "custom, unreviewed");
    assert!(unsigned.explanation().contains("permissions you give it"));

    let bundle_only = Trust::signed(
        "owner/gmx-ndi",
        Signature {
            level: Level::Bundle,
            digest: "abcdef0123456789abcdef".into(),
            identity: None,
            issuer: None,
            log_index: Some(1),
        },
    );
    assert_eq!(bundle_only.label(), "signed, digest only");
    assert!(bundle_only.explanation().contains("cosign"));

    let full = Trust::signed(
        "owner/gmx-ndi",
        Signature {
            level: Level::Cosign,
            digest: "abc".into(),
            identity: Some("https://github.com/psmux/godwinmix-plugins/.github/workflows/listing.yml@refs/heads/main".into()),
            issuer: None,
            log_index: Some(1),
        },
    );
    assert_eq!(full.label(), "signed");
    assert!(full.explanation().contains("cosign verified"));
}

#[test]
fn what_was_asked_for_and_what_it_resolved_to_are_kept_apart() {
    // An update refetches `source`. If the tag had been written there, an
    // unpinned install would refetch the release it already has for ever,
    // which is the bug this field split exists to prevent.
    let trust = Trust::unsigned("psmux/gmx-ndi", "no signature").resolved_to("v1.2.0");
    assert_eq!(trust.source, "psmux/gmx-ndi");
    assert_eq!(trust.resolved, "v1.2.0");
    assert_eq!(trust.origin(), "psmux/gmx-ndi (v1.2.0)");

    // A source that resolved to itself reads as itself.
    let path = Trust::unsigned("/opt/gmx/clock", "a local path");
    assert_eq!(path.origin(), "/opt/gmx/clock");
}

#[test]
fn a_trust_record_survives_a_round_trip_to_disk() {
    let dir = temp("record");
    let trust = Trust::unsigned("./clock", "a local path");
    trust.write(&dir).expect("it writes");
    assert_eq!(Trust::read(&dir), Some(trust));
    let _ = std::fs::remove_dir_all(&dir);
}
