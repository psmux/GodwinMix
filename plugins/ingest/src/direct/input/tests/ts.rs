//! MPEG-TS over UDP, RTP, SRT and RIST.

#[cfg(target_os = "macos")]
use gstreamer::prelude::*;
use serde_json::json;

use super::*;

const LAVFI_V: &str = "testsrc2=size=320x240:rate=25";

/// Multicast on the loopback interface, joined by name, H.264 and AAC. The
/// sender leaves by `lo0` the way `udp/output` sets it, because ffmpeg
/// cannot send multicast from the loopback address.
#[cfg(target_os = "macos")]
#[test]
fn multicast_on_a_named_interface_carries_h264_and_aac_untouched() {
    let _one = one_at_a_time();
    if !which("ffprobe") {
        return;
    }
    let tx = Local::launch(&live_ts("udpsink name=out host=239.255.19.21 port=19921 auto-multicast=false sync=false"));
    gmx_udp::iface::send_multicast_out(&tx.0.by_name("out").unwrap(), "lo0").expect("lo0 is there");
    let rx = start(json!({"uri": "udp://@239.255.19.21:19921", "params": {"interface": "lo0"}}), &Context::default());
    assert!(eventually(15, || rx.got.keyframes() >= 4), "only {} keyframes arrived", rx.got.keyframes());
    let s = rx.got.last();
    assert_eq!((s.video_codec.as_str(), s.width, s.height), ("h264", 320, 240), "{s:?}");
    assert_eq!((s.audio_codec.as_str(), s.audio_channels), ("aac", 1), "audiotestsrc is mono: {s:?}");
    assert_eq!(s.state, crate::direct::input::stats::State::Live, "{s:?}");
    assert!(s.kbps > 0 && (20.0..30.0).contains(&s.fps) && s.keyframe_ms.is_some_and(|k| (900..1100).contains(&k)), "{s:?}");
    let streams = probe(&rx.got, "multicast");
    assert!(streams.iter().any(|s| s.0 == "h264" && s.1 >= 50 && s.2 == 320), "{streams:?}");
    assert!(streams.iter().any(|s| s.0 == "aac" && s.1 > 50), "{streams:?}");
}

/// RTP wrapped TS carrying HEVC and 5.1 AC-3: the enhanced RTMP bodies
/// read back with ffprobe, AC-3 frames and all.
#[test]
fn rtp_wrapped_hevc_and_ac3_come_through_as_enhanced_tags() {
    let _one = one_at_a_time();
    if !which("ffmpeg") || !which("ffprobe") {
        return;
    }
    let _tx = spawn("ffmpeg", &["-loglevel", "error", "-re", "-f", "lavfi", "-i", LAVFI_V, "-f", "lavfi", "-i", "sine=frequency=440",
        "-t", "20", "-c:v", "libx265", "-preset", "ultrafast", "-x265-params", "keyint=25:log-level=error", "-c:a", "ac3", "-ac", "6",
        "-f", "rtp_mpegts", "rtp://127.0.0.1:19922"]);
    let rx = start(json!("rtp://127.0.0.1:19922"), &Context::default());
    assert!(eventually(20, || rx.got.keyframes() >= 3 && rx.got.frames(TagKind::Audio) > 40), "{} keyframes", rx.got.keyframes());
    let s = rx.got.last();
    assert_eq!((s.video_codec.as_str(), s.width, s.height), ("h265", 320, 240), "{s:?}");
    assert_eq!((s.audio_codec.as_str(), s.audio_channels), ("ac3", 6), "{s:?}");
    let streams = probe(&rx.got, "hevc-ac3");
    assert!(streams.iter().any(|s| s.0 == "hevc" && s.1 >= 40), "{streams:?}");
    assert!(streams.iter().any(|s| s.0 == "ac3" && s.1 >= 40 && s.4 == 6), "{streams:?}");
}

/// Two programs, named in the SDT; program 2 is chosen by number, and its
/// MPEG layer II audio is carried and named.
#[test]
fn a_program_is_chosen_from_a_multiplex_and_every_program_is_named() {
    let _one = one_at_a_time();
    if !which("ffmpeg") || !which("ffprobe") {
        return;
    }
    let _tx = spawn("ffmpeg", &["-loglevel", "error", "-re",
        "-f", "lavfi", "-i", LAVFI_V, "-f", "lavfi", "-i", "sine=frequency=440",
        "-f", "lavfi", "-i", "testsrc2=size=352x288:rate=25", "-f", "lavfi", "-i", "sine=frequency=880",
        "-map", "0:v", "-map", "1:a", "-map", "2:v", "-map", "3:a", "-t", "20",
        "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-c:a:0", "aac", "-c:a:1", "mp2", "-b:a:1", "192k",
        "-program", "program_num=1:title=Sport:st=0:st=1", "-program", "program_num=2:title=News:st=2:st=3",
        "-f", "mpegts", "udp://127.0.0.1:19923?pkt_size=1316"]);
    let rx = start(json!({"uri": "udp://127.0.0.1:19923", "program": 2}), &Context::default());
    assert!(eventually(15, || rx.got.keyframes() >= 3 && rx.got.last().programs.len() == 2), "{:?}", rx.got.last());
    let s = rx.got.last();
    let names: Vec<(u16, &str)> = s.programs.iter().map(|p| (p.number, p.name.as_str())).collect();
    assert_eq!(names, [(1, "Sport"), (2, "News")]);
    assert_eq!(s.program, Some(2));
    assert_eq!((s.width, s.height), (352, 288), "program 2's picture: {s:?}");
    assert_eq!((s.audio_codec.as_str(), s.audio_channels), ("mp2", 1), "{s:?}");
    let streams = probe(&rx.got, "program-2");
    assert!(streams.iter().any(|s| s.0 == "h264" && s.2 == 352), "{streams:?}");
    // FLV's FourCC for MPEG audio is `.mp3`, so ffprobe names the codec by
    // it; its decoder reads the layer from each frame and decodes them all.
    assert!(streams.iter().any(|s| s.0 == "mp3" && s.1 > 20), "{streams:?}");
}

/// Two percent of datagrams dropped on the way: counted as continuity
/// errors and lost packets, and the pictures keep coming.
#[test]
fn loss_is_counted_and_the_input_keeps_going_through_it() {
    let _one = one_at_a_time();
    if !which("gst-launch-1.0") {
        return;
    }
    let _tx = gst(&live_ts("identity drop-probability=0.02 ! udpsink host=127.0.0.1 port=19924 sync=false"));
    let rx = start(json!("udp://127.0.0.1:19924"), &Context::default());
    assert!(eventually(20, || rx.got.keyframes() >= 6 && rx.got.last().cc_errors > 3), "{:?}", rx.got.last());
    let s = rx.got.last();
    assert!(s.packets_lost >= s.cc_errors, "{s:?}");
    assert_eq!(s.state, crate::direct::input::stats::State::Live, "{s:?}");
    let times = rx.got.times();
    assert!(times.windows(2).all(|w| w[0] <= w[1] + 1000), "time ran backwards");
}

/// SRT both ways: a listener a sender calls, and a caller dialling out to a
/// sender that listens.
#[test]
fn srt_listens_for_a_caller_and_calls_a_listener() {
    let _one = one_at_a_time();
    if !which("gst-launch-1.0") {
        return;
    }
    let send = |target: &str| gst(&live_ts(&format!("srtsink uri={target} wait-for-connection=false")));
    let rx = start(json!({"uri": "srt://@:19925", "params": {"latency_ms": 120}}), &Context::default());
    std::thread::sleep(Duration::from_millis(300));
    let _a = send("srt://127.0.0.1:19925?mode=caller");
    assert!(eventually(15, || rx.got.keyframes() >= 3), "listener: {:?}", rx.got.last());
    assert_eq!(rx.got.last().video_codec, "h264");
    drop(rx);
    let _b = send("srt://:19926?mode=listener");
    let rx = start(json!("srt://127.0.0.1:19926"), &Context::default());
    assert!(eventually(20, || rx.got.keyframes() >= 3), "caller: {:?}", rx.got.last());
    assert!(rx.got.last().kbps > 0);
}

/// RIST Simple Profile, the sender pushing to the input's even port.
#[test]
fn rist_carries_the_feed() {
    let _one = one_at_a_time();
    if !which("gst-launch-1.0") {
        return;
    }
    let rx = start(json!("rist://@0.0.0.0:19928"), &Context::default());
    // In this process, so what the sender's bus said is in the message.
    let tx = Local::launch(&live_ts("rtpmp2tpay ! ristsink address=127.0.0.1 port=19928"));
    // Until the stats say what came, not only until the pictures have: the
    // stats are published once a second, and on a macOS runner three
    // keyframes were in while the last stats still read connecting.
    let arrived = eventually(15, || rx.got.keyframes() >= 3 && !rx.got.last().audio_codec.is_empty());
    assert!(arrived, "{} keyframes, {:?}; the sender said {:?}", rx.got.keyframes(), rx.got.last(), said(&tx.0));
    let s = rx.got.last();
    assert_eq!((s.video_codec.as_str(), s.audio_codec.as_str()), ("h264", "aac"), "{s:?}");
}
