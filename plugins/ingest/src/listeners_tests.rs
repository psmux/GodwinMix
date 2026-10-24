use super::*;
use crate::hub::Hub;
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::RwLock;

fn free_tcp() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn free_udp() -> u16 {
    UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn listeners(rtmp: u16, srt: u16) -> Listeners {
    let settings = Settings::from_params(&json!({"bind": "127.0.0.1", "rtmp_port": rtmp, "srt_port": srt}));
    let gate = Arc::new(ChannelGate {
        hub: Hub::new(),
        table: Arc::new(RwLock::new(Table::default())),
        open_app: String::new(),
        relay: RwLock::default(),
        reporter: None,
        on_air: Default::default(),
    });
    Listeners::new(&settings, gate)
}

fn table(channels: Value) -> Table {
    Table::from_params(&json!({ "channels": channels }))
}

fn tcp_open(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

fn udp_taken(port: u16) -> bool {
    UdpSocket::bind(("127.0.0.1", port)).is_err()
}

fn row<'a>(rows: &'a [Value], protocol: &str) -> &'a Value {
    rows.iter().find(|r| r["protocol"] == protocol).unwrap_or_else(|| panic!("no {protocol} row in {rows:?}"))
}

#[test]
fn nothing_is_open_until_a_channel_asks_and_it_closes_when_the_last_one_goes() {
    let (rtmp, srt) = (free_tcp(), free_udp());
    let mut l = listeners(rtmp, srt);
    l.apply(&Table::default());
    assert!(!tcp_open(rtmp), "no channel, no RTMP port");
    assert!(!udp_taken(srt), "no channel, no SRT port");

    l.apply(&table(json!([{"id": "a", "keys": [], "protocols": ["rtmp"]}])));
    assert!(tcp_open(rtmp), "an RTMP channel opens the RTMP port");
    assert!(!udp_taken(srt));
    let rows = l.rows();
    assert_eq!(row(&rows, "rtmp")["open"], true);
    assert_eq!(row(&rows, "rtmp")["because"], json!(["a"]));

    l.apply(&table(json!([{"id": "a", "keys": [], "protocols": ["rtmp", "srt"]}])));
    if crate::srt::ffi_available() {
        assert!(udp_taken(srt), "switching SRT on opens the SRT port");
        assert_eq!(row(&l.rows(), "srt")["open"], true);
    }

    l.apply(&Table::default());
    assert!(l.rows().iter().all(|r| r["open"] == false), "every listener closed with the last channel");
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while udp_taken(srt) && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(!udp_taken(srt), "the SRT port closed with the last channel");
}

#[test]
fn a_channel_without_rtmp_keeps_the_port_on_the_loopback_for_the_mixers_own_sources() {
    let srt = free_udp();
    let mut l = listeners(0, srt);
    l.apply(&table(json!([{"id": "a", "keys": [], "protocols": ["whip"]}])));
    let rows = l.rows();
    assert_eq!(row(&rows, "rtmp")["open"], false);
    assert_eq!(row(&rows, "relay")["open"], true);
    assert_eq!(row(&rows, "relay")["loopback"], true);
    assert!(tcp_open(l.rtmp_port()), "the hub is still reachable from this machine");
}

#[test]
fn rtmps_without_a_certificate_says_what_to_do_and_opens_nothing() {
    let (rtmp, srt, secure) = (free_tcp(), free_udp(), free_tcp());
    let mut l = listeners(rtmp, srt);
    l.apply(&table(json!([{"id": "a", "keys": [], "protocols": ["rtmp"], "rtmps_port": secure}])));
    let rows = l.rows();
    let rtmps = row(&rows, "rtmps");
    assert_eq!(rtmps["open"], false);
    assert!(rtmps["problem"].as_str().unwrap_or("").contains("certificate"), "{rtmps}");
    assert!(!tcp_open(secure));
}

/// What a loopback client hears back when it asks the server at `addr`
/// which one it is: the server's token, or nothing.
fn who_answers(addr: &str) -> String {
    use std::io::{BufRead, BufReader, Write};
    let Ok(mut s) = TcpStream::connect(addr) else { return String::new() };
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let _ = s.write_all(crate::rtmp::WHO);
    let mut line = String::new();
    let _ = BufReader::new(s).read_line(&mut line);
    line.trim_end().to_string()
}

#[test]
fn the_relay_is_the_rtmp_port_when_loopback_reaches_this_plugin() {
    let rtmp = free_tcp();
    let mut l = listeners(rtmp, free_udp());
    l.readers = vec!["ch101".into()];
    l.apply(&Table::default());
    assert_eq!(l.gate.relay_address(), format!("127.0.0.1:{rtmp}"));
    assert!(who_answers(&l.gate.relay_address()).starts_with("gmx-"), "the relay answers as this plugin");
}

#[test]
fn a_port_another_program_holds_on_loopback_is_never_taken_for_the_relay() {
    // Another program on 127.0.0.1:<port>, as an ingest plugin left running
    // from an earlier start was. On Linux the wide bind is refused; on
    // Windows it can succeed and loopback still reaches the other program.
    // Either way the relay must be a port that reaches this plugin.
    let other = TcpListener::bind("127.0.0.1:0").unwrap();
    let taken = other.local_addr().unwrap().port();
    let settings = Settings::from_params(&json!({"bind": "0.0.0.0", "rtmp_port": taken, "srt_port": free_udp()}));
    let mut l = Listeners::new(&settings, listeners(0, 0).gate.clone());
    l.readers = vec!["ch101".into()];
    l.apply(&table(json!([{"id": "a", "keys": [], "protocols": ["rtmp"]}])));
    let relay = l.gate.relay_address();
    assert!(!relay.is_empty(), "the shows on this machine still have a relay");
    assert_ne!(relay, format!("127.0.0.1:{taken}"), "the relay is not the other program's port");
    assert!(who_answers(&relay).starts_with("gmx-"), "{relay} answers as this plugin");
    let rows = l.rows();
    assert_eq!(row(&rows, "relay")["open"], true, "{rows:?}");
    assert!(rows.iter().any(|r| r["problem"].is_string()), "the rows say what is wrong: {rows:?}");
    drop(other);
}
