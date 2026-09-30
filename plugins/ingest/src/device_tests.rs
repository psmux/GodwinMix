use super::*;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn table() -> Table {
    Table::from_params(&json!({"channels": [
        {"id": "church", "app": "church", "enabled": true, "key_mode": "query",
         "keys": [{"id": "obs", "secret": "s3cret"}]},
    ]}))
}

fn a_device(table: Table) -> Discover {
    // The open door is asked for, so the tests that use it have a port with
    // no channels; with channels it makes no difference.
    let settings = Settings::from_params(&json!({"bind": "127.0.0.1", "rtmp_port": 0, "open_door": true}));
    Discover::start(&settings, table, None).expect("the loopback has a free port")
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(program)).find(|c| c.is_file())
}

/// Three seconds of a small test picture with sound, published to `url`.
fn publish(url: &str) -> Option<Child> {
    let ffmpeg = which("ffmpeg")?;
    Command::new(ffmpeg)
        .args(["-loglevel", "error", "-re", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440", "-t", "4"])
        .args(["-c:v", "libx264", "-g", "15", "-preset", "ultrafast", "-c:a", "aac"])
        // The RTMPS test's certificate is self signed, which is the point.
        .args(if url.starts_with("rtmps:") { &["-tls_verify", "0"][..] } else { &[][..] })
        .args(["-f", "flv", url])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .ok()
}

fn wait_for(what: impl Fn() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(8);
    while Instant::now() < until {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn a_device_binds_and_reports_that_nobody_is_publishing() {
    let device = a_device(Table::default());
    assert!(device.port() > 0);
    assert!(device.candidates().is_empty());
    let health = device.health();
    assert!(health.detail.unwrap_or_default().contains("nobody publishing"));
}

#[test]
fn a_dry_run_says_what_it_would_do_and_needs_no_core() {
    let device = a_device(Table::default());
    let _live = device.hub().publish("live", "phone", "10.0.0.9:51000", None).unwrap();
    let result = device.add_publishers(&json!({"dry_run": true}), Err("no core in a test".into()));
    assert_eq!(result.is_error, Some(false));
    let plan = result.structured_content.expect("a plan");
    assert_eq!(plan["add"], json!(["live-phone"]));
    let candidates = device.candidates();
    assert_eq!(candidates[0].params["stream"], "live/phone");
    assert_eq!(candidates[0].params["relay"], format!("127.0.0.1:{}", device.port()));
}

#[test]
fn a_real_run_with_no_core_reachable_is_an_error_that_says_why() {
    let device = a_device(Table::default());
    let result = device.add_publishers(&json!({}), Err("GMX_RPC is empty".to_string()));
    assert_eq!(result.is_error, Some(true));
    assert!(result.content.to_string().contains("GMX_RPC"));
}

#[test]
fn two_streams_on_one_channel_are_live_at_once_and_a_wrong_key_is_turned_away() {
    let device = a_device(table());
    let port = device.port();
    let Some(mut main) = publish(&format!("rtmp://127.0.0.1:{port}/church/main?psk=s3cret")) else {
        eprintln!("skipping: no ffmpeg on PATH");
        return;
    };
    let mut cam2 = publish(&format!("rtmp://127.0.0.1:{port}/church/cam2?key=s3cret")).unwrap();
    let mut wrong = publish(&format!("rtmp://127.0.0.1:{port}/church/main2?psk=nope")).unwrap();
    let both = wait_for(|| device.hub().is_live("church", "main") && device.hub().is_live("church", "cam2"));
    let refused = wrong.wait().map(|s| !s.success()).unwrap_or(false);
    let stats = device.streams().structured_content.unwrap();
    let _ = main.kill();
    let _ = cam2.kill();
    let _ = (main.wait(), cam2.wait());
    assert!(both, "both streams were live together: {stats}");
    assert!(refused, "ffmpeg with a wrong key must fail");
    assert!(!device.hub().is_live("church", "main2"));
    let main = stats["streams"].as_array().unwrap().iter().find(|s| s["stream"] == "main").unwrap().clone();
    assert_eq!(main["key"], "obs");
}

#[test]
fn a_hub_reader_on_the_same_port_gets_flv_with_the_headers_first() {
    let device = a_device(table());
    let port = device.port();
    let mut reader = crate::relay::request(&format!("127.0.0.1:{port}"), "church/main").unwrap();
    let Some(mut publisher) = publish(&format!("rtmp://127.0.0.1:{port}/church/main?psk=s3cret")) else {
        eprintln!("skipping: no ffmpeg on PATH");
        return;
    };
    reader.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
    let mut got = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    while got.len() < 20_000 {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => got.extend_from_slice(&chunk[..n]),
        }
    }
    let described = device.hub().stream("church", "main");
    let _ = publisher.kill();
    let _ = publisher.wait();
    assert!(got.len() >= 20_000, "only {} bytes came through the relay", got.len());
    assert_eq!(&got[0..3], b"FLV");
    // After the 13 byte file header: onMetaData, then the AVC header.
    assert_eq!(got[13], 18, "the first tag is the metadata");
    let first_len = u32::from_be_bytes([0, got[14], got[15], got[16]]) as usize;
    let second = 13 + 11 + first_len + 4;
    assert_eq!(got[second], 9, "then the video sequence header");
    assert_eq!(&got[second + 11..second + 13], &[0x17, 0x00]);
    let video = described.expect("live")["video"].clone();
    assert_eq!(video["codec"], "h264");
    assert_eq!(video["width"], 320);
    assert_eq!(video["height"], 240);
}

#[test]
fn taking_a_key_back_cuts_off_the_publisher_on_air_with_it() {
    let device = a_device(table());
    let port = device.port();
    let Some(mut publisher) = publish(&format!("rtmp://127.0.0.1:{port}/church/main?psk=s3cret")) else {
        eprintln!("skipping: no ffmpeg on PATH");
        return;
    };
    assert!(wait_for(|| device.hub().is_live("church", "main")), "it went live");
    device.set_table(Table::from_params(&json!({"channels": [
        {"id": "church", "app": "church", "keys": [{"id": "other", "secret": "different"}]}]})));
    let cut = wait_for(|| !device.hub().is_live("church", "main"));
    let _ = publisher.kill();
    let _ = publisher.wait();
    assert!(cut, "the stream on the key that was taken back is still live");
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

#[test]
fn with_no_channels_nothing_listens_and_a_channel_opens_the_port_it_needs() {
    let port = free_port();
    let settings = Settings::from_params(&json!({"bind": "127.0.0.1", "rtmp_port": port}));
    let device = Discover::start(&settings, Table::default(), None).expect("nothing to bind");
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err(), "no channel, no port");
    device.set_table(table());
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok(), "an RTMP channel opened it");
    let rows = device.listeners();
    assert!(rows.iter().any(|r| r["protocol"] == "rtmp" && r["open"] == true && r["because"] == json!(["church"])), "{rows:?}");
    device.set_table(Table::default());
    // Asked of the device rather than of the port: another test running at
    // the same time may be handed the port number the moment it is free.
    let rows = device.listeners();
    assert!(rows.iter().all(|r| r["open"] == false), "closed with the last channel: {rows:?}");
}

#[test]
fn rtmps_on_its_own_port_lets_a_publisher_in_over_tls_into_the_same_hub() {
    let secure = free_port();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).expect("a certificate");
    let table = Table::from_params(&json!({
        "channels": [{"id": "church", "app": "church", "protocols": [], "rtmps_port": secure,
                      "keys": [{"id": "obs", "secret": "s3cret"}]}],
        "tls": {"cert": cert.cert.pem(), "key": cert.key_pair.serialize_pem()},
    }));
    let device = a_device(table);
    let Some(mut publisher) = publish(&format!("rtmps://127.0.0.1:{secure}/church/main?psk=s3cret")) else {
        eprintln!("skipping: no ffmpeg on PATH");
        return;
    };
    let live = wait_for(|| device.hub().is_live("church", "main"));
    let described = device.hub().stream("church", "main");
    let _ = publisher.kill();
    let said = publisher.wait_with_output().map(|o| String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default();
    assert!(live, "an RTMPS publisher went live: {said}");
    assert_eq!(described.unwrap()["protocol"], "rtmps");
}
