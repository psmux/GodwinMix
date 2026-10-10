//! An RTMP output and the MPEG-TS outputs (SRT, RIST) on one programme, added
//! in either order. The RTMP parser settled the shared encoder on `avc`, and
//! an SRT output added after it was refused: "Pads do not have common format".

use super::slow_output::silent_server;
use super::*;

/// A UDP port nothing was using a moment ago.
fn udp_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// An even one, as RIST wants, with the one above it free too.
fn even_udp_port() -> u16 {
    loop {
        let p = udp_port();
        if p % 2 == 0 && std::net::UdpSocket::bind(("127.0.0.1", p + 1)).is_ok() {
            return p;
        }
    }
}

fn addresses() -> [(&'static str, String); 3] {
    [
        ("to-rtmp", format!("rtmp://127.0.0.1:{}/live/key", silent_server())),
        // A listener with nobody calling takes the stream and drops it, so
        // bytes reach the sink without a receiver on this machine.
        ("to-srt", format!("srt://127.0.0.1:{}?mode=listener", udp_port())),
        ("to-rist", format!("rist://127.0.0.1:{}", even_udp_port())),
    ]
}

/// Bytes past the muxer of every output in `ids` within `within`, or the
/// ones that sent none.
async fn all_sending(handle: &MixerHandle, ids: &[&str], within: Duration) -> Vec<String> {
    let until = Instant::now() + within;
    loop {
        let status = handle.status().await.expect("status answers");
        let quiet: Vec<String> = ids
            .iter()
            .filter(|id| {
                let out = status.outputs.iter().find(|o| o.id == **id);
                out.and_then(|o| o.extra.get("bytes_out")).and_then(|v| v.as_u64()).unwrap_or(0) == 0
            })
            .map(|s| s.to_string())
            .collect();
        if quiet.is_empty() || Instant::now() > until {
            return quiet;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn in_order(order: &[usize]) {
    let _ = gst::init();
    let needed = ["srtsink", "ristsink", "rtmp2sink", "mpegtsmux", "flvmux"];
    if !needed.iter().all(|e| crate::probe::exists(e)) {
        eprintln!("skipping: needs {needed:?}");
        return;
    }
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.hardware.encode = crate::config::Accel::Software;
    cfg.hardware.decode = crate::config::Accel::Software;
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    let cfg: SourceConfig = toml::from_str("id = \"bars\"\nuri = \"test://smpte\"\n").unwrap();
    mix.add_source(&cfg, None).unwrap();
    let all = addresses();
    let first = &all[order[0]];
    mix.add_output(&OutputConfig::bare(first.0, &first.1)).expect("the first output attaches");
    // Long enough for the first output to settle the encoder's caps, which
    // is what the next one has to live with.
    std::thread::sleep(Duration::from_secs(3));
    for i in &order[1..] {
        let (id, uri) = &all[*i];
        if let Err(e) = mix.add_output(&OutputConfig::bare(id, uri)) {
            panic!("{id} did not attach after {}: {e:#}", first.0);
        }
    }
    let thread = spawn(mix, cmd_rx, handle.clone());
    let within = Duration::from_secs(15).mul_f64(crate::plugin::harness::timing_slack());
    let ids: Vec<&str> = all.iter().map(|a| a.0).collect();
    let quiet = all_sending(&handle, &ids, within).await;
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert!(quiet.is_empty(), "no bytes left these outputs: {quiet:?} (order {order:?})");
}

#[tokio::test(flavor = "multi_thread")]
async fn srt_and_rist_attach_after_rtmp() {
    in_order(&[0, 1, 2]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn rtmp_attaches_after_srt_and_rist() {
    in_order(&[1, 2, 0]).await;
}
