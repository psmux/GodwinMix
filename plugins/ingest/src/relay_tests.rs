use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::media_tag::{MediaTag, TagKind};

fn tag(kind: TagKind, track: u8, ms: u32, body: &[u8]) -> MediaTag {
    let keyframe = kind == TagKind::Video && body[0] == 0x17;
    MediaTag { track, kind, timestamp_ms: ms, keyframe, sequence_header: body.get(1) == Some(&0), payload: Arc::from(body) }
}

/// Every tag's (type, stream id) the relay wrote for `line`.
fn read(hub: &Hub, line: &str) -> Vec<(u8, u8)> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client.write_all(line.as_bytes()).unwrap();
    let (conn, _) = listener.accept().unwrap();
    let h = hub.clone();
    std::thread::spawn(move || serve(&h, conn, &[]));
    std::thread::sleep(Duration::from_millis(200));
    let p = hub.publish("direct.x", "main", "udp", None).unwrap();
    p.push(tag(TagKind::Video, 0, 0, &[0x17, 0, 0, 0, 0, 1]));
    p.push(tag(TagKind::Audio, 0, 0, &[0xaf, 0, 0x11, 0x90]));
    p.push(tag(TagKind::Video, 0, 0, &[0x17, 1, 0, 0, 0, 9]));
    p.push(tag(TagKind::Audio, 2, 0, &[0x91, b'a', b'c', b'-', b'3', 0x0b, 0x77]));
    p.push(tag(TagKind::Audio, 0, 10, &[0xaf, 1, 7]));
    drop(p);
    client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let mut got = Vec::new();
    let _ = client.read_to_end(&mut got);
    let (mut at, mut out) = (13, Vec::new());
    while at + 11 <= got.len() {
        let len = u32::from_be_bytes([0, got[at + 1], got[at + 2], got[at + 3]]) as usize;
        out.push((got[at], got[at + 10]));
        at += 11 + len + 4;
    }
    out
}

#[test]
fn a_reader_that_asks_for_every_track_gets_each_one_numbered_and_others_get_track_0() {
    let hub = Hub::new();
    let plain = read(&hub, "GMXHUB direct.x/main\n");
    assert_eq!(plain, [(9, 0), (8, 0), (9, 0), (8, 0)]);
    let all = read(&hub, &format!("GMXHUB direct.x/main {ALL_TRACKS}\n"));
    assert_eq!(all, [(9, 0), (8, 0), (9, 0), (8, 2), (8, 0)]);
}
