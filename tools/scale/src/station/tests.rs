//! The bulk add against a stand in station: one that has `show.add_many`,
//! and one that has only `show.add`, each answering in the contract's shape.

use super::add;
use super::csv::parse;
use super::rpc::Client;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// Serves `answers` connections in order, sending each request's line and
/// body back on the channel.
fn station(methods: Value, answers: Vec<Value>) -> (String, mpsc::Receiver<(String, Value)>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let api = json!({"methods": methods});
        for answer in std::iter::once(api).chain(answers) {
            let (mut s, _) = l.accept().unwrap();
            let mut raw = Vec::new();
            let mut buf = [0u8; 65536];
            loop {
                let n = s.read(&mut buf).unwrap();
                raw.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&raw).to_string();
                let Some(split) = text.find("\r\n\r\n") else { continue };
                let len: usize = text.lines().find_map(|l| l.strip_prefix("Content-Length: ")).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
                if raw.len() >= split + 4 + len {
                    let body = serde_json::from_slice(&raw[split + 4..]).unwrap_or(Value::Null);
                    tx.send((text.lines().next().unwrap_or("").to_string(), body)).unwrap();
                    break;
                }
            }
            let out = answer.to_string();
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{out}", out.len());
        }
    });
    (addr, rx)
}

fn method(name: &str, verb: &str, path: &str) -> Value {
    json!({"name": name, "rest": {"method": verb, "path": path}})
}

const LIST: &str = "name,input,program,output,format\nfeed-001,udp://@239.77.0.1:5000,2,udp://127.0.0.1:30000,copy\nfeed-002,udp://@239.77.0.2:5000,1,udp://127.0.0.1:30001,youtube-720p30\n";

#[test]
fn bulk_add_dry_runs_then_applies() {
    let methods = json!([method("show.add_many", "POST", "/api/v1/shows/add_many")]);
    let plan = json!({"plan": {"cost": {}, "fits": true}, "added": [], "refused": []});
    let applied = json!({"added": ["feed-001", "feed-002"], "refused": [], "plan": {"fits": true}});
    let (addr, rx) = station(methods, vec![plan, applied]);
    let c = Client::connect(&addr, None).unwrap();
    let out = add::run(&c, &parse(LIST).unwrap(), false).unwrap();
    let _api = rx.recv().unwrap();
    let (line, dry) = rx.recv().unwrap();
    assert_eq!(line, "POST /api/v1/shows/add_many HTTP/1.1");
    assert_eq!(dry["dry_run"], true);
    let first = &dry["shows"][0];
    assert_eq!(first["compositing"], false);
    assert_eq!(first["input"], json!({"uri": "udp://@239.77.0.1:5000", "program": 2}));
    assert_eq!(first["outputs"][0]["rendition"], Value::Null, "copy is no rendition");
    assert_eq!(dry["shows"][1]["outputs"][0]["rendition"], json!({"preset": "youtube-720p30"}));
    assert_eq!(rx.recv().unwrap().1["dry_run"], false);
    assert_eq!(out["added"], 2);
    assert_eq!(out["ids"], json!(["feed-001", "feed-002"]));
    assert_eq!(out["plan"]["fits"], true);
}

#[test]
fn without_add_many_each_row_is_added_and_an_old_station_is_named() {
    let methods = json!([method("show.add", "POST", "/api/v1/shows")]);
    let made = json!({"id": "feed-001", "compositing": false, "state": "starting"});
    let (addr, rx) = station(methods.clone(), vec![made.clone(), json!({"id": "feed-002", "compositing": false})]);
    let c = Client::connect(&addr, None).unwrap();
    let out = add::run(&c, &parse(LIST).unwrap(), false).unwrap();
    assert_eq!((out["method"].as_str(), out["added"].as_u64()), (Some("show.add"), Some(2)));
    assert_eq!(rx.iter().nth(1).unwrap().1["input"]["program"], 2);
    let (addr, _rx) = station(methods, vec![json!({"id": "feed-001", "state": "starting"})]);
    let c = Client::connect(&addr, None).unwrap();
    let out = add::run(&c, &parse(LIST).unwrap(), false).unwrap();
    assert_eq!(out["added"], 0);
    assert!(out["refused"][0]["why"].as_str().unwrap().contains("--legacy"));
}
