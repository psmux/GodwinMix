//! The receiver against real sockets on this machine. Senders are either
//! `gst-launch-1.0` making real H.264 in MPEG-TS, or a `UdpSocket` sending
//! packets built by hand when the test needs to control every byte.

use super::*;
use crate::ts::tests::packet;
use crate::ts::{header, packetize, tables, PACKET};
use godwinmix_sdk::wire::HealthState;
use serde_json::json;
use std::net::UdpSocket;
use std::time::Duration;

fn free_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn temp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("gmx-udp-{name}-{}.ts", std::process::id()))
}

fn receiver(params: serde_json::Value, file: &std::path::Path) -> Receiver {
    let s = Settings::from_params(&params).expect("good settings");
    Receiver::start(&s, None, Sink::File(file.to_path_buf())).expect("the receiver starts")
}

/// Wait up to five seconds for `done`.
fn eventually(mut done: impl FnMut() -> bool) -> bool {
    for _ in 0..50 {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// A two program multiplex, one datagram of seven packets per call, with
/// the counters running on from `round`.
fn mux_datagrams(round: u8) -> Vec<Vec<u8>> {
    let mut all = Vec::new();
    let pat = [0x00, 0xB0, 0x11, 0, 1, 0xC1, 0, 0, 0, 1, 0xF0, 0x01, 0, 2, 0xF0, 0x02];
    let mut pat = pat.to_vec();
    let crc = crate::ts::crc32(&pat);
    pat.extend_from_slice(&crc.to_be_bytes());
    packetize(0, &pat, &mut round.clone(), &mut all);
    for (number, pmt_pid, es) in [(1u16, 0x1001u16, 256u16), (2, 0x1002, 512)] {
        let p = tables::Program { number, pmt_pid, pcr_pid: es, ..Default::default() };
        let s = tables::Stream { pid: es, stream_type: 0x1B, info: vec![] };
        packetize(pmt_pid, &tables::build_pmt(&p, 0, &[&s]), &mut round.clone(), &mut all);
    }
    for pid in [256u16, 512, 0x1FFF, 256] {
        all.extend(packet(pid, round, false, &[0xAA]));
    }
    all.chunks(7 * PACKET).map(<[u8]>::to_vec).collect()
}

#[test]
fn one_program_is_chosen_out_of_two_and_the_stuffing_is_gone() {
    let (port, file) = (free_port(), temp("choose"));
    let r = receiver(json!({"address": "127.0.0.1", "port": port, "program": 2}), &file);
    let tx = UdpSocket::bind("127.0.0.1:0").unwrap();
    for round in 0..20u8 {
        for d in mux_datagrams(round) {
            tx.send_to(&d, ("127.0.0.1", port)).unwrap();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(eventually(|| std::fs::metadata(&file).map(|m| m.len() > 0).unwrap_or(false)));
    std::thread::sleep(Duration::from_millis(300));
    let programs = r.programs();
    drop(r);
    let bytes = std::fs::read(&file).unwrap();
    let _ = std::fs::remove_file(&file);
    let pids: Vec<u16> = bytes.chunks(PACKET).map(|p| header(p).unwrap().pid).collect();
    assert!(pids.contains(&512) && !pids.contains(&256) && !pids.contains(&0x1FFF), "{pids:?}");
    assert_eq!(programs["chosen"], 2, "{programs}");
    assert_eq!(programs["programs"].as_array().unwrap().len(), 2);
}

#[test]
fn rtp_is_unwrapped_and_a_missing_sequence_number_is_counted() {
    let (port, file) = (free_port(), temp("rtp"));
    let r = receiver(json!({"uri": format!("rtp://127.0.0.1:{port}")}), &file);
    let tx = UdpSocket::bind("127.0.0.1:0").unwrap();
    for (i, seq) in [1u16, 2, 3, 5, 6, 9].iter().enumerate() {
        let mut d = vec![0x80, 33];
        d.extend_from_slice(&seq.to_be_bytes());
        d.extend_from_slice(&[0; 8]);
        d.extend(packet(256, i as u8, false, &[1]));
        tx.send_to(&d, ("127.0.0.1", port)).unwrap();
    }
    assert!(eventually(|| r.stats()["rtp_packets_lost"] == 3), "{}", r.stats());
    drop(r);
    let _ = std::fs::remove_file(&file);
}

#[test]
fn a_unicast_port_already_taken_is_refused_with_what_to_do() {
    let taken = UdpSocket::bind("0.0.0.0:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let s = Settings::from_params(&json!({"port": port})).unwrap();
    let err = match Receiver::start(&s, None, Sink::File(temp("taken"))) {
        Ok(r) => {
            // Some platforms let the bind through and fail on the first read.
            assert!(eventually(|| r.health().state == HealthState::Failing), "{:?}", r.health());
            return;
        }
        Err(e) => e,
    };
    assert!(err.contains("Choose another port"), "{err}");
}

/// The picture a real encoder makes, through a real socket.
#[test]
fn a_real_encoder_on_the_loopback_arrives_and_reports_ok() {
    let Some(launcher) = which("gst-launch-1.0") else {
        eprintln!("skipping: gst-launch-1.0 is not on PATH");
        return;
    };
    let (port, file) = (free_port(), temp("real"));
    let r = receiver(json!({"address": "127.0.0.1", "port": port}), &file);
    let mut sender = std::process::Command::new(launcher)
        .args(["-q", "videotestsrc", "is-live=true", "!", "video/x-raw,width=320,height=240,framerate=30/1",
               "!", "x264enc", "tune=zerolatency", "key-int-max=30", "!", "mpegtsmux", "alignment=7", "!",
               "udpsink", "host=127.0.0.1", &format!("port={port}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("gst-launch-1.0 starts");
    let ok = eventually(|| r.health().state == HealthState::Ok && std::fs::metadata(&file).map(|m| m.len() > 50_000).unwrap_or(false));
    let health = r.health();
    let _ = sender.kill();
    let _ = sender.wait();
    drop(r);
    let _ = std::fs::remove_file(&file);
    assert!(ok, "{health:?}");
}

/// Send `count` copies of one datagram to a group, out of the named interface.
#[cfg(target_os = "macos")]
/// `udpsink` with the interface set the way `udp/output` sets it, because the
/// standard library cannot choose the interface multicast leaves by.
fn multicast_out(iface: &str, group: &str, port: u16, count: usize, d: &[u8]) {
    let line = format!(
        "appsrc name=in format=bytes ! udpsink name=out host={group} port={port} multicast-iface={iface} \
         auto-multicast=false sync=false"
    );
    let pipeline = gst::parse::launch(&line).unwrap().downcast::<gst::Pipeline>().unwrap();
    let src = pipeline.by_name("in").unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let sink = pipeline.by_name("out").unwrap();
    crate::iface::send_multicast_out(&sink, iface).expect("the interface is set");
    for _ in 0..count {
        let _: gst::FlowReturn = src.emit_by_name("push-buffer", &[&gst::Buffer::from_slice(d.to_vec())]);
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(200));
    let _ = pipeline.set_state(gst::State::Null);
}

/// Two receivers on one group and port, joined on the loopback interface by
/// name, both get the feed. It is also the test that the interface setting is
/// honoured at both ends: the datagrams leave by `lo0` and nowhere else.
/// macOS only: Linux's `lo` carries multicast only when it has the MULTICAST
/// flag, which is not the default, and Windows names interfaces differently.
#[cfg(target_os = "macos")]
#[test]
fn two_receivers_of_one_group_on_a_named_interface_both_get_it() {
    let lo = "lo0";
    let port = free_port();
    let (a, b) = (temp("group-a"), temp("group-b"));
    let params = json!({"address": "239.255.71.1", "port": port, "interface": lo});
    let (ra, rb) = (receiver(params.clone(), &a), receiver(params, &b));
    let mut d = Vec::new();
    packetize(0, &tables::build_pat(1, 0, 1, 0x1000), &mut 0, &mut d);
    let p = tables::Program { number: 1, pmt_pid: 0x1000, pcr_pid: 256, ..Default::default() };
    let s = tables::Stream { pid: 256, stream_type: 0x1B, info: vec![] };
    packetize(0x1000, &tables::build_pmt(&p, 0, &[&s]), &mut 0, &mut d);
    multicast_out(lo, "239.255.71.1", port, 25, &d);
    let both = eventually(|| ra.stats()["datagrams"] == 25 && rb.stats()["datagrams"] == 25);
    let (sa, sb) = (ra.stats(), rb.stats());
    drop((ra, rb));
    let _ = (std::fs::remove_file(&a), std::fs::remove_file(&b));
    assert!(both, "a: {sa}, b: {sb}");
}

/// macOS refuses a join on an interface that does not exist, and the refusal
/// says to check the name. What Linux does with an unknown name has not been
/// tried, so the test runs on macOS only.
#[cfg(target_os = "macos")]
#[test]
fn an_interface_that_does_not_exist_is_refused_with_what_to_check() {
    let s = Settings::from_params(&json!({"address": "239.255.71.2", "port": free_port(), "interface": "nosuch0"})).unwrap();
    let err = match Receiver::start(&s, None, Sink::File(temp("nosuch"))) {
        Ok(r) => {
            assert!(eventually(|| r.health().state == HealthState::Failing), "{:?}", r.health());
            r.health().detail.unwrap_or_default()
        }
        Err(e) => e,
    };
    assert!(err.contains("Check the interface name"), "{err}");
}

pub fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(program)).find(|c| c.is_file())
}
