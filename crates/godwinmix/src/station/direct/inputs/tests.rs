use super::place::{query, with_query};
use super::*;
use godwinmix_protocol::shows::BackupInput;
use serde_json::{json, Value};

fn input(uri: &str, params: Option<Value>, backup: Option<&str>) -> InputSpec {
    InputSpec {
        uri: uri.into(),
        program: None,
        params: params.and_then(|p| p.as_object().cloned()),
        backup: backup.map(|b| BackupInput { uri: b.into(), program: None, params: None }),
    }
}

fn show(name: &str) -> String {
    format!("test-input-{name}-{}", std::process::id())
}

#[test]
fn a_passphrase_in_the_address_is_sealed_kept_as_the_sentinel_and_unsealed_for_the_host() {
    let id = show("uri");
    let given = input("srt://feed:9000?mode=caller&passphrase=correct-horse-battery&latency=200", None, Some("srt://spare:9000?passphrase=spare-passphrase-1"));
    assert!(in_clear(&given));
    let kept = seal(&id, given.clone()).unwrap();
    assert_eq!(kept.uri, "srt://feed:9000?mode=caller&passphrase=__secret__&latency=200");
    assert_eq!(kept.backup.as_ref().unwrap().uri, "srt://spare:9000?passphrase=__secret__");
    assert!(!in_clear(&kept));
    let text = serde_json::to_string(&kept).unwrap();
    assert!(!text.contains("correct-horse") && !text.contains("spare-passphrase"), "{text}");
    assert_eq!(unsealed(&id, kept.clone()), given, "the host opens what was written");

    // Sent back as a list showed it: the sealed passphrase stays.
    let again = seal(&id, kept.clone()).unwrap();
    assert_eq!(unsealed(&id, again), given);
    // Taken out: forgotten, and the host gets an address without one.
    let none = seal(&id, input("srt://feed:9000?mode=caller", None, None)).unwrap();
    assert_eq!(unsealed(&id, kept).uri, "srt://feed:9000?mode=caller&latency=200");
    assert_eq!(unsealed(&id, none).uri, "srt://feed:9000?mode=caller");
    forget(&id);
}

#[test]
fn a_passphrase_in_params_is_sealed_the_same_way_and_other_params_are_left_alone() {
    let id = show("params");
    let given = input("srt://feed:9000", Some(json!({"passphrase": "in-the-params-123", "latency": 120})), None);
    let kept = seal(&id, given.clone()).unwrap();
    assert_eq!(kept.params.as_ref().unwrap()["passphrase"], SENTINEL);
    assert_eq!(kept.params.as_ref().unwrap()["latency"], 120);
    assert_eq!(unsealed(&id, kept), given);
    let plain = input("udp://@239.1.1.1:5000", None, None);
    assert_eq!(seal(&id, plain.clone()).unwrap(), plain, "nothing to seal, nothing changed");
    forget(&id);
}

#[test]
fn an_address_without_a_query_or_a_passphrase_is_untouched() {
    assert_eq!(with_query("srt://a:1", Some("x")), "srt://a:1");
    assert_eq!(with_query("srt://a:1?passphrase=p", None), "srt://a:1");
    assert_eq!(query("srt://a:1?latency=5"), None);
}
