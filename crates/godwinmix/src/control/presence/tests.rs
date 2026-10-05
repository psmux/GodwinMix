//! Presence without a server: the device guess, and a seat given back.

use super::*;
use godwinmix_protocol::scope::Token;

#[test]
fn a_user_agent_reads_as_a_device_a_person_recognises() {
    let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 \
                  (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
    assert_eq!(device_of(iphone), "iPhone Safari");
    let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like \
                Gecko) Chrome/120.0 Safari/537.36 Edg/120.0";
    assert_eq!(device_of(edge), "Windows Edge");
    assert_eq!(device_of("tungstenite-rs"), "tungstenite-rs");
    assert_eq!(device_of(""), "");
}

#[test]
fn a_seat_is_given_back_when_the_connection_goes() {
    let presence = Presence::new();
    let mut told = presence.subscribe();
    let token = Token::open();
    let here = presence.join("open.s1", &token, None, Some("gmx/0.2"));
    assert!(told.try_recv().is_ok(), "joining was not announced");
    assert!(presence.set("open.s1", Some("wide".into()), Some("Sam".into())));
    let list = presence.list(Some("open.s1"));
    assert_eq!(list.clients.len(), 1);
    assert!(list.clients[0].you);
    assert_eq!(list.clients[0].scene.as_deref(), Some("wide"));
    assert_eq!(presence.who("open.s1").as_deref(), Some("Sam (gmx CLI)"));
    drop(here);
    assert!(presence.list(None).clients.is_empty(), "the seat outlived its connection");
    assert!(!presence.set("open.s1", None, None), "nobody is there to describe");
}
