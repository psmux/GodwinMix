use super::*;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-control-tls-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_made_certificate_is_kept_sealed_and_used_again_on_the_next_start() {
    let dir = scratch("kept");
    let store = Secrets::open(&dir.join("secrets")).unwrap();
    let public = dir.join("godwinmix.control.crt");
    let wanted = names(&["localhost", "127.0.0.1", "192.168.1.20"]);
    let first = obtain(&ControlTls::default(), &dir, &store, &public, &wanted).unwrap();
    assert_eq!(first.source, Source::SelfSigned);
    assert_eq!(first.names, wanted);
    assert_eq!(std::fs::read_to_string(&public).unwrap(), first.pair.cert, "the public half is on disk");
    let sealed = std::fs::read_to_string(dir.join("secrets").join("store.json")).unwrap();
    assert!(!sealed.contains("PRIVATE KEY"), "the key is sealed, not written out");

    let reopened = Secrets::open(&dir.join("secrets")).unwrap();
    let again = obtain(&ControlTls::default(), &dir, &reopened, &public, &wanted[..2]).unwrap();
    assert_eq!(again.pair, first.pair, "a subset of the names it covers keeps the same certificate");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_address_makes_a_new_certificate_that_covers_it() {
    let dir = scratch("moved");
    let store = Secrets::open(&dir.join("secrets")).unwrap();
    let public = dir.join("c.crt");
    let first = obtain(&ControlTls::default(), &dir, &store, &public, &names(&["localhost", "10.0.0.5"])).unwrap();
    let moved = obtain(&ControlTls::default(), &dir, &store, &public, &names(&["localhost", "10.0.0.9"])).unwrap();
    assert_ne!(moved.pair, first.pair);
    assert_eq!(moved.names, names(&["localhost", "10.0.0.9"]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_operators_own_files_are_read_relative_to_the_config() {
    let dir = scratch("files");
    let store = Secrets::open(&dir.join("secrets")).unwrap();
    let pair = tls_cert::for_server(&names(&["mixer.example.com"]), "test").unwrap();
    std::fs::write(dir.join("mixer.crt"), &pair.cert).unwrap();
    std::fs::write(dir.join("mixer.key"), &pair.key).unwrap();
    let tls = ControlTls { enabled: true, cert: Some("mixer.crt".into()), key: Some("mixer.key".into()) };
    let loaded = obtain(&tls, &dir, &store, &dir.join("unused.crt"), &[]).unwrap();
    assert_eq!(loaded.source, Source::Files);
    assert_eq!(loaded.pair, pair);
    assert!(!dir.join("unused.crt").exists(), "nothing is written for the operator's own");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_file_or_half_a_pair_says_what_to_do() {
    let dir = scratch("broken");
    let store = Secrets::open(&dir.join("secrets")).unwrap();
    let missing = ControlTls { enabled: true, cert: Some("nope.crt".into()), key: Some("nope.key".into()) };
    let why = obtain(&missing, &dir, &store, &dir.join("c.crt"), &[]).unwrap_err().to_string();
    assert!(why.contains("nope.crt") && why.contains("cannot be read") && why.contains("Fix the path"), "{why}");
    let half = ControlTls { enabled: true, cert: Some("a.crt".into()), key: None };
    let why = obtain(&half, &dir, &store, &dir.join("c.crt"), &[]).unwrap_err().to_string();
    assert!(why.contains("only one of cert and key"), "{why}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_key_that_does_not_match_the_certificate_is_refused_naming_both_files() {
    let dir = scratch("crossed");
    let store = Secrets::open(&dir.join("secrets")).unwrap();
    let one = tls_cert::for_server(&names(&["a.example"]), "a").unwrap();
    let two = tls_cert::for_server(&names(&["b.example"]), "b").unwrap();
    std::fs::write(dir.join("a.crt"), &one.cert).unwrap();
    std::fs::write(dir.join("b.key"), &two.key).unwrap();
    let tls = ControlTls { enabled: true, cert: Some("a.crt".into()), key: Some("b.key".into()) };
    let why = format!("{:#}", obtain(&tls, &dir, &store, &dir.join("c.crt"), &[]).unwrap_err());
    assert!(why.contains("a.crt") && why.contains("b.key") && why.contains("do not go together"), "{why}");
    let _ = std::fs::remove_dir_all(&dir);
}
