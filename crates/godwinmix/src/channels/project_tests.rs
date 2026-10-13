//! A channel's feed through a project file and back, on real registries:
//! a pull's address with keys included and without, and a programme fed
//! channel whose key the file left out.

use serde_json::json;

use super::super::newkey::typed_tests::{add, open, scratch};
use super::read;

const URL: &str = "rtmp://user:pw@cdn.example:1935/live/sekret-key";

fn carry(from: &super::Channels, to: &super::Channels, secrets: bool) -> Vec<String> {
    let incoming = from.project_export(secrets).iter().map(|v| read(v, &|_, _| false).unwrap().0).collect();
    to.project_import(incoming, false).unwrap()
}

fn pulled(dir: &std::path::Path) -> std::sync::Arc<super::Channels> {
    let channels = open(dir);
    channels.add(add(json!({"name": "Relay", "feed": {"kind": "pull", "url": URL}}))).unwrap();
    channels
}

#[tokio::test]
async fn a_pull_address_travels_with_the_keys() {
    let (a, b) = (scratch("project-pull-a"), scratch("project-pull-b"));
    let (from, to) = (pulled(&a), open(&b));
    assert!(from.project_export(true)[0].to_string().contains("sekret-key"), "carried when keys are asked for");
    let waiting = carry(&from, &to, true);
    assert!(waiting.is_empty(), "{waiting:?}");
    let record = to.records.lock().iter().find(|r| r.id == "relay").cloned().unwrap();
    assert_eq!(to.pull_url(&record).as_deref(), Some(URL));
    let file = std::fs::read_to_string(b.join("godwinmix.runtime.channels.toml")).unwrap();
    assert!(!file.contains("sekret"), "sealed again, not written in the channels file: {file}");
    for d in [a, b] {
        std::fs::remove_dir_all(&d).ok();
    }
}

#[tokio::test]
async fn without_the_keys_a_pull_names_its_host_and_waits_for_the_address() {
    let (a, b) = (scratch("project-pull-nokeys-a"), scratch("project-pull-nokeys-b"));
    let (from, to) = (pulled(&a), open(&b));
    let file = from.project_export(false)[0].to_string();
    assert!(!file.contains("sekret") && !file.contains("pw@"), "{file}");
    let waiting = carry(&from, &to, false);
    assert!(waiting.iter().any(|w| w.contains("pulls from rtmp://cdn.example:1935")), "{waiting:?}");
    let record = to.records.lock().iter().find(|r| r.id == "relay").cloned().unwrap();
    assert_eq!(to.pull_url(&record), None);
    for d in [a, b] {
        std::fs::remove_dir_all(&d).ok();
    }
}

#[tokio::test]
async fn a_programme_fed_channel_without_its_key_gets_a_new_one_and_is_fed() {
    let (a, b) = (scratch("project-programme-a"), scratch("project-programme-b"));
    let (from, to) = (open(&a), open(&b));
    from.add(add(json!({"name": "Live", "feed": {"kind": "programme"}}))).unwrap();
    carry(&from, &to, false);
    let feeds = to.programme_feeds();
    assert_eq!(feeds.len(), 1, "the programme still reaches the channel");
    assert!(feeds[0].uri.contains("?psk="), "{}", feeds[0].uri);
    let keys = to.get("live").unwrap().keys;
    assert_eq!(keys.iter().filter(|k| k.label == "Programme").count(), 1, "{keys:?}");
    for d in [a, b] {
        std::fs::remove_dir_all(&d).ok();
    }
}
