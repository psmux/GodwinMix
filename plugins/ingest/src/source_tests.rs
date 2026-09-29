use super::*;
use serde_json::json;

#[test]
fn the_defaults_wait_on_1935_for_any_key() {
    let s = Settings::from_params(&json!({}));
    assert_eq!(s.port, 1935);
    assert!(s.app.is_empty());
    assert!(s.stream_key.is_empty());
    assert!(s.problem().is_none());
    assert!(s.publish_url(1935).contains("<any key>"));
}

#[test]
fn a_configured_app_and_key_show_up_in_the_address_to_hand_out() {
    let s = Settings::from_params(&json!({"app": "live", "stream_key": "phone"}));
    assert_eq!(s.publish_url(1935), "rtmp://<this machine>:1935/live/phone");
}

#[test]
fn a_relay_that_is_not_a_host_port_is_refused_with_the_way_out() {
    let s = Settings::from_params(&json!({"relay": "nonsense"}));
    let problem = s.problem().expect("a relay needs a port");
    assert!(problem.contains("ingest/discover"), "{problem}");
}

#[test]
fn a_listener_on_an_ephemeral_port_comes_up_and_says_nobody_is_publishing() {
    let path = std::env::temp_dir().join(format!("gmx-ingest-{}.flv", std::process::id()));
    let settings = Settings::from_params(&json!({"bind": "127.0.0.1", "port": 0}));
    let ingest = Ingest::start(&settings, None, Out::File(path.clone()))
        .expect("the loopback has a free port");
    assert!(ingest.port() > 0);
    let health = ingest.health();
    assert_eq!(health.state, godwinmix_sdk::wire::HealthState::Degraded);
    assert!(health.detail.unwrap_or_default().contains("rtmp://"));
    assert_eq!(ingest.stats()["bytes"], 0);
    drop(ingest);
    let _ = std::fs::remove_file(&path);
}

/// The repository has no `which` crate; this is the same four lines the
/// core's own tests use.
fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

#[test]
fn a_real_publisher_arrives_and_its_stream_reaches_the_pipe_as_matroska() {
    let Some(launcher) = which("gst-launch-1.0") else {
        eprintln!("skipping: gst-launch-1.0 is not on PATH");
        return;
    };
    if gmx_netkit::init().is_err()
        || !gmx_netkit::elements::exists("rtmp2sink")
        || !gmx_netkit::elements::exists("x264enc")
    {
        eprintln!("skipping: this build of GStreamer cannot publish RTMP");
        return;
    }
    let path = std::env::temp_dir().join(format!("gmx-ingest-live-{}.mkv", std::process::id()));
    let settings = Settings::from_params(&json!({"bind": "127.0.0.1", "port": 0}));
    let ingest =
        Ingest::start(&settings, None, Out::File(path.clone())).expect("the listener starts");
    let port = ingest.port();

    let mut publisher = std::process::Command::new(launcher)
        .args([
            "-q",
            "videotestsrc",
            "is-live=true",
            "!",
            "video/x-raw,width=320,height=240,framerate=30/1",
            "!",
            "x264enc",
            "tune=zerolatency",
            "key-int-max=15",
            "!",
            "h264parse",
            "!",
            "flvmux",
            "streamable=true",
            "!",
            "rtmp2sink",
            &format!("location=rtmp://127.0.0.1:{port}/live/test"),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("gst-launch-1.0 starts");

    let mut bytes = 0u64;
    let mut publishing = false;
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        publishing = ingest.health().state == godwinmix_sdk::wire::HealthState::Ok;
        if bytes > 20_000 && publishing {
            break;
        }
    }
    let stats = ingest.stats();
    let head = std::fs::read(&path).unwrap_or_default();
    let _ = publisher.kill();
    let _ = publisher.wait();
    drop(ingest);
    let _ = std::fs::remove_file(&path);

    assert!(publishing, "the listener never saw a publisher: {stats}");
    assert!(bytes > 20_000, "only {bytes} bytes arrived");
    // The EBML magic. The publisher's FLV is remuxed to Matroska here; the
    // module comment in src/remux.rs says why.
    assert_eq!(
        &head[0..4],
        &[0x1a, 0x45, 0xdf, 0xa3],
        "what came out is not a Matroska stream"
    );
    assert!(stats["publishing"].as_str().unwrap_or("").contains("live/test"));

    // The header being right is not the same as the stream being openable.
    // The core's container transport is `fdsrc ! decodebin`, so this runs
    // the same decodebin over what came out and insists it decodes.
    let kept = std::env::temp_dir().join(format!("gmx-ingest-kept-{}.mkv", std::process::id()));
    std::fs::write(&kept, &head).expect("keep the capture for the decode check");
    let decoded = std::process::Command::new(which("gst-launch-1.0").expect("launcher"))
        .args([
            "-q",
            "filesrc",
            &format!("location={}", kept.display()),
            "!",
            "decodebin",
            "!",
            "fakesink",
        ])
        .output()
        .expect("gst-launch-1.0 runs");
    let _ = std::fs::remove_file(&kept);
    assert!(
        decoded.status.success(),
        "decodebin would not open the stream this plugin produced: {}",
        String::from_utf8_lossy(&decoded.stderr)
    );
}

#[test]
fn a_channel_server_that_is_not_up_yet_is_waited_for_rather_than_refused() {
    // The mixer restores its sources before it starts its plugins, so a
    // source a scene held across a restart asks before anything listens.
    // Refusing to start would lose the source; it waits and says so instead.
    let path = std::env::temp_dir().join(format!("gmx-ingest-r-{}.flv", std::process::id()));
    let settings = Settings::from_params(&json!({"relay": "127.0.0.1:1", "stream": "live/x"}));
    let ingest = Ingest::start(&settings, None, Out::File(path.clone()))
        .expect("a channel server that is not up yet is waited for");
    let health = ingest.health();
    assert_eq!(health.state, godwinmix_sdk::wire::HealthState::Degraded);
    assert!(health.detail.unwrap_or_default().contains("live/x"));
    drop(ingest);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_relay_without_a_stream_name_is_refused_before_anything_connects() {
    let s = Settings::from_params(&json!({"relay": "127.0.0.1:1935"}));
    let problem = s.problem().expect("a relay needs a stream");
    assert!(problem.contains("<channel>/<stream>"), "{problem}");
}

#[test]
fn a_source_reading_a_channel_stream_from_the_hub_gets_matroska() {
    let Some(ffmpeg) = which("ffmpeg") else {
        eprintln!("skipping: no ffmpeg on PATH");
        return;
    };
    let table = crate::channels::Table::from_params(&json!({"channels": [
        {"id": "church", "app": "church", "keys": [{"id": "obs", "secret": "k"}]}]}));
    let device_settings = crate::device::Settings::from_params(&json!({"bind": "127.0.0.1", "rtmp_port": 0}));
    let device = crate::device::Discover::start(&device_settings, table, None).expect("device");
    let port = device.port();
    let path = std::env::temp_dir().join(format!("gmx-ingest-hub-{}.mkv", std::process::id()));
    let settings = Settings::from_params(&json!({"relay": format!("127.0.0.1:{port}"), "stream": "church/main"}));
    let ingest = Ingest::start(&settings, None, Out::File(path.clone())).expect("the source reaches the hub");
    let mut publisher = std::process::Command::new(ffmpeg)
        .args(["-loglevel", "error", "-re", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440", "-t", "5", "-c:v", "libx264", "-g", "15"])
        .args(["-preset", "ultrafast", "-c:a", "aac", "-f", "flv"])
        .arg(format!("rtmp://127.0.0.1:{port}/church/main?psk=k"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("ffmpeg starts");
    let mut bytes = 0;
    for _ in 0..80 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if bytes > 20_000 {
            break;
        }
    }
    let health = ingest.health();
    let _ = publisher.kill();
    let _ = publisher.wait();
    drop(ingest);
    let head = std::fs::read(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    assert!(bytes > 20_000, "only {bytes} bytes arrived through the hub");
    assert_eq!(&head[0..4], &[0x1a, 0x45, 0xdf, 0xa3], "not Matroska");
    assert_eq!(health.state, godwinmix_sdk::wire::HealthState::Ok, "{:?}", health.detail);
}
