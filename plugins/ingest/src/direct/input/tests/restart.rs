//! A sender that stops and a new one that starts on the same port: with new
//! PIDs, a new program number, a stream added to the PMT, or the same layout
//! with its clock started again. The input follows each one.

use serde_json::json;

use super::*;

/// H.264 and AAC from gst-launch over UDP, the video on `vpid` and the
/// audio on `apid`, in program `program`.
fn sender(port: u16, vpid: u16, apid: u16, program: u16) -> Sender {
    sender_to(&format!("udpsink host=127.0.0.1 port={port} sync=false"), vpid, apid, program)
}

/// The same into `sink`, a gst-launch fragment.
fn sender_to(sink: &str, vpid: u16, apid: u16, program: u16) -> Sender {
    gst(&format!(
        "mpegtsmux name=m alignment=7 prog-map=program_map,sink_{vpid}={program},sink_{apid}={program} ! {sink} \
         videotestsrc is-live=true ! video/x-raw,width=320,height=240,framerate=25/1 ! x264enc tune=zerolatency \
         speed-preset=ultrafast key-int-max=25 ! h264parse ! queue ! m.sink_{vpid} \
         audiotestsrc is-live=true ! audioconvert ! avenc_aac ! aacparse ! queue ! m.sink_{apid}"
    ))
}

/// The same from ffmpeg, its PIDs starting at `start_pid`, with a second
/// audio stream when `extra_audio`.
fn ffmpeg_sender(port: u16, start_pid: u16, program: u16, extra_audio: bool) -> Sender {
    let (pid, prog) = (format!("0x{start_pid:x}"), program.to_string());
    let mut args = vec!["-loglevel", "error", "-re", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-f", "lavfi", "-i", "sine=frequency=440"];
    args.extend_from_slice(&["-map", "0:v", "-map", "1:a"]);
    if extra_audio {
        args.extend_from_slice(&["-map", "1:a"]);
    }
    args.extend_from_slice(&["-t", "30", "-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency", "-g", "25", "-c:a", "aac",
        "-mpegts_start_pid", &pid, "-mpegts_service_id", &prog, "-f", "mpegts"]);
    let to = format!("udp://127.0.0.1:{port}?pkt_size=1316");
    args.push(&to);
    spawn("ffmpeg", &args)
}

/// Frames keep arriving after a restart: fifty more video frames within
/// `secs`, the input reads live again, and its time neither runs back nor
/// leaves a hole.
///
/// The message says whether the sender is still running: on a macOS runner
/// the input heard nothing for 34 s after the third sender started, and
/// whether that sender had died was not in the message.
fn follows(rx: &Running, tx: &mut Sender, what: &str, secs: u64) {
    let before = rx.got.frames(TagKind::Video);
    let more = || rx.got.frames(TagKind::Video) - before;
    let arrived = eventually(secs, || more() >= 50);
    let sender = match tx.0.try_wait() {
        Ok(None) => "still running".to_string(),
        Ok(Some(status)) => format!("ended with {status}"),
        Err(e) => format!("could not be asked: {e}"),
    };
    assert!(arrived, "{what}: {} frames after the restart, the sender {sender}, {:?}", more(), rx.got.last());
    assert!(eventually(3, || rx.got.last().state == crate::direct::input::stats::State::Live), "{what}: {:?}", rx.got.last());
    let times = rx.got.times();
    let back = times.windows(2).map(|w| w[0].saturating_sub(w[1])).max().unwrap_or(0);
    let gap = times.windows(2).map(|w| w[1].saturating_sub(w[0])).max().unwrap_or(0);
    assert!(back < 1_000, "{what}: the video's time ran back by {back} ms");
    assert!(gap < 3_000, "{what}: a gap of {gap} ms in the video's time");
}

#[test]
fn a_restarted_gst_sender_is_followed_with_new_pids_or_the_same_ones() {
    let _one = one_at_a_time();
    if !which("gst-launch-1.0") {
        return;
    }
    let port = 19931;
    let rx = start(json!(format!("udp://127.0.0.1:{port}")), &Context::default());
    let tx = sender(port, 65, 66, 1);
    assert!(eventually(15, || rx.got.keyframes() >= 3), "the first sender: {:?}", rx.got.last());
    drop(tx);
    let mut tx = sender(port, 65, 66, 1);
    follows(&rx, &mut tx, "the same layout again", 12);
    drop(tx);
    let mut tx = sender(port, 300, 301, 7);
    follows(&rx, &mut tx, "new PIDs and a new program", 12);
    assert!(eventually(3, || rx.got.last().programs.iter().map(|p| p.number).eq([7])), "the programs read again: {:?}", rx.got.last());
}

#[test]
fn an_srt_caller_that_calls_again_with_new_pids_is_followed() {
    let _one = one_at_a_time();
    if !which("gst-launch-1.0") {
        return;
    }
    let rx = start(json!({"uri": "srt://@:19934", "params": {"latency_ms": 120}}), &Context::default());
    std::thread::sleep(Duration::from_millis(300));
    let to = "srtsink uri=srt://127.0.0.1:19934?mode=caller wait-for-connection=false";
    let tx = sender_to(to, 65, 66, 1);
    assert!(eventually(15, || rx.got.keyframes() >= 3), "the first caller: {:?}", rx.got.last());
    drop(tx);
    let mut tx = sender_to(to, 300, 301, 7);
    follows(&rx, &mut tx, "a new caller with new PIDs", 15);
}

#[test]
fn a_restarted_ffmpeg_sender_is_followed_with_new_pids_and_a_new_stream() {
    let _one = one_at_a_time();
    if !which("ffmpeg") {
        return;
    }
    let port = 19932;
    let rx = start(json!(format!("udp://127.0.0.1:{port}")), &Context::default());
    let tx = ffmpeg_sender(port, 0x100, 1, false);
    assert!(eventually(15, || rx.got.keyframes() >= 3), "the first sender: {:?}", rx.got.last());
    drop(tx);
    let mut tx = ffmpeg_sender(port, 0x200, 1, true);
    follows(&rx, &mut tx, "new PIDs and a second audio stream", 12);
    let left_out = rx.got.last().error.unwrap_or_default();
    assert!(left_out.contains("a second audio stream"), "the second audio is named: {left_out}");
    drop(tx);
    let mut tx = ffmpeg_sender(port, 0x200, 1, true);
    follows(&rx, &mut tx, "the same layout again", 12);
}
