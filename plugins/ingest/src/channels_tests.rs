use super::*;
use serde_json::json;

fn table() -> Table {
    Table::from_params(&json!({"channels": [
        {"id": "sunday-service", "app": "sunday-service", "enabled": true, "key_mode": "query",
         "keys": [{"id": "obs-laptop", "secret": "k3y-one"}, {"id": "phone", "secret": "k3y-two"}]},
        {"id": "youth", "app": "youth", "enabled": true, "key_mode": "stream",
         "keys": [{"id": "hall-encoder", "secret": "hall-secret"}]},
        {"id": "off", "app": "off", "enabled": false, "keys": [{"id": "a", "secret": "b"}]},
    ]}))
}

#[test]
fn every_spelling_of_the_key_is_let_in_and_the_stream_keeps_its_name() {
    for query in ["psk", "key", "token", "Token"] {
        let admit = table()
            .admit("sunday-service", &format!("main_720p?{query}=k3y-two"))
            .unwrap_or_else(|r| panic!("{query}: {}", r.why));
        assert_eq!(admit.stream, "main_720p");
        assert_eq!(admit.key, "phone");
        assert_eq!(admit.channel, "sunday-service");
    }
}

#[test]
fn a_key_on_the_application_name_is_found_too() {
    let admit = table().admit("sunday-service?psk=k3y-one", "cam2").expect("let in");
    assert_eq!((admit.app.as_str(), admit.stream.as_str()), ("sunday-service", "cam2"));
}

#[test]
fn a_key_that_is_the_stream_name_is_named_after_its_id_and_never_shown() {
    let admit = table().admit("youth", "hall-secret").expect("let in");
    assert_eq!(admit.stream, "hall-encoder");
    let refused = table().admit("youth", "wrong-secret").unwrap_err();
    assert_eq!(refused.stream, "", "a wrong key must not reach a log line or an event");
    assert!(!refused.why.contains("wrong-secret"));
}

#[test]
fn each_refusal_says_what_to_do_next() {
    let t = table();
    let none = t.admit("sunday-service", "main").unwrap_err();
    assert!(none.why.contains("?psk="), "{}", none.why);
    assert_eq!(none.stream, "main");
    let wrong = t.admit("sunday-service", "main?psk=nope").unwrap_err();
    assert!(wrong.why.contains("Channels page"), "{}", wrong.why);
    assert!(!wrong.why.contains("nope"));
    let off = t.admit("off", "main?psk=b").unwrap_err();
    assert!(off.why.contains("switched off"), "{}", off.why);
    let lost = t.admit("elsewhere", "main?psk=b").unwrap_err();
    assert_eq!(lost.channel, "elsewhere");
    assert!(lost.why.contains("server address"), "{}", lost.why);
}

#[test]
fn no_channels_is_the_open_door_discover_always_was() {
    assert!(Table::from_params(&json!({})).is_open());
    assert!(Table::from_params(&json!({"channels": []})).is_open());
    assert!(!table().is_open());
}

#[test]
fn keys_are_compared_whole() {
    assert!(same("abc", "abc"));
    assert!(!same("abc", "abd"));
    assert!(!same("abc", "abcd"));
}

fn moved_from_livebox() -> Table {
    Table::from_params(&json!({"channels": [
        {"id": "church", "app": "Church", "enabled": true, "key_mode": "query",
         "keys": [{"id": "livebox", "secret": "Sunday-2024"}]},
        {"id": "youth-hall", "app": "Youth Hall", "enabled": true, "key_mode": "query",
         "keys": [{"id": "livebox", "secret": "my hall pw"}]},
    ]}))
}

#[test]
fn an_application_name_is_matched_whatever_its_case_and_named_the_channels_way() {
    for sent in ["Church", "church", "CHURCH"] {
        let admit = moved_from_livebox()
            .admit(sent, "main?psk=Sunday-2024")
            .unwrap_or_else(|r| panic!("{sent}: {}", r.why));
        assert_eq!(admit.channel, "church");
        assert_eq!(admit.app, "Church", "the hub and the core see one spelling");
    }
}

#[test]
fn a_space_arrives_as_typed_escaped_or_as_a_plus() {
    let t = moved_from_livebox();
    for (app, key) in [("Youth Hall", "my hall pw"), ("Youth%20Hall", "my%20hall%20pw"), ("youth hall", "my+hall+pw")] {
        let admit = t.admit(app, &format!("main?psk={key}")).unwrap_or_else(|r| panic!("{app} {key}: {}", r.why));
        assert_eq!((admit.channel.as_str(), admit.app.as_str()), ("youth-hall", "Youth Hall"));
    }
    assert!(t.admit("Youth Hall", "main?psk=my hall").is_err(), "a key is still compared whole");
}
