//! What one input costs at 1080p, 8 Mbit/s: the CPU this process spends
//! while an input carries the feed, with the sender in another process.
//! Run by hand, in release:
//!
//! ```sh
//! cargo test --release -p gmx-ingest -- --ignored cpu_per_input --nocapture --test-threads=1
//! ```

use serde_json::json;

use super::*;

/// This process's CPU time so far, in seconds, as `ps` reports it.
fn cpu_seconds() -> f64 {
    let out = Command::new("ps").args(["-o", "time=", "-p", &std::process::id().to_string()]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    text.trim().split(':').fold(0.0, |acc, part| acc * 60.0 + part.parse::<f64>().unwrap_or(0.0))
}

/// Carry `spec` for `secs` after a warm up, and answer the CPU it took as a
/// share of one core, with the last numbers it reported.
fn measure(spec: Value, secs: u64) -> (f64, InputStats) {
    let rx = start(spec, &Context::default());
    std::thread::sleep(Duration::from_secs(4));
    let (before, at) = (cpu_seconds(), Instant::now());
    std::thread::sleep(Duration::from_secs(secs));
    let used = (cpu_seconds() - before) / at.elapsed().as_secs_f64();
    let last = rx.got.last();
    drop(rx);
    (used, last)
}

/// Twenty seconds of 1080p25 H.264 at 8 Mbit/s and AAC, in CBR MPEG-TS.
fn hd_clip() -> PathBuf {
    clip("1080p-8mbit.ts", &["-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=25", "-f", "lavfi", "-i", "sine", "-t", "20",
        "-c:v", "libx264", "-preset", "veryfast", "-b:v", "8M", "-maxrate", "8M", "-bufsize", "8M", "-g", "50",
        "-c:a", "aac", "-b:a", "192k", "-f", "mpegts", "-muxrate", "8800k"])
}

fn copy_out(clip: &Path, to: &[&str]) -> Sender {
    let c = path(clip);
    let mut args = vec!["-loglevel", "error", "-re", "-stream_loop", "-1", "-i", &c, "-c", "copy"];
    args.extend_from_slice(to);
    spawn("ffmpeg", &args)
}

#[test]
#[ignore]
fn cpu_per_input_at_1080p_8_mbit() {
    let _one = one_at_a_time();
    if !which("ffmpeg") || !which("gst-launch-1.0") {
        return;
    }
    let clip = hd_clip();
    let mut rows = Vec::new();
    {
        let _tx = copy_out(&clip, &["-f", "mpegts", "udp://127.0.0.1:19936?pkt_size=1316"]);
        rows.push(("udp", measure(json!("udp://127.0.0.1:19936"), 15)));
    }
    {
        let _tx = copy_out(&clip, &["-f", "rtp_mpegts", "rtp://127.0.0.1:19937"]);
        rows.push(("rtp", measure(json!("rtp://127.0.0.1:19937"), 15)));
    }
    {
        let _tx = copy_out(&clip, &["-f", "mpegts", "udp://127.0.0.1:19939?pkt_size=1316"]);
        let _relay = gst("udpsrc port=19939 ! srtsink uri=srt://127.0.0.1:19925?mode=caller wait-for-connection=false");
        rows.push(("srt", measure(json!("srt://@:19925"), 15)));
    }
    {
        let _tx = copy_out(&clip, &["-f", "mpegts", "udp://127.0.0.1:19940?pkt_size=1316"]);
        let _relay = gst("udpsrc port=19940 caps=video/mpegts ! rtpmp2tpay ! ristsink address=127.0.0.1 port=19928");
        rows.push(("rist", measure(json!("rist://@0.0.0.0:19928"), 15)));
    }
    rows.push(("file", measure(json!(format!("file://{}", path(&clip))), 15)));
    {
        let _server = copy_out(&clip, &["-f", "flv", "-listen", "1", "rtmp://127.0.0.1:19934/live/feed"]);
        std::thread::sleep(Duration::from_millis(800));
        rows.push(("rtmp", measure(json!("rtmp://127.0.0.1:19934/live/feed"), 15)));
    }
    {
        let dir = scratch().join("hls-hd");
        std::fs::create_dir_all(&dir).unwrap();
        let out = path(&dir.join("live.m3u8"));
        let _encoder = copy_out(&clip, &["-f", "hls", "-hls_time", "2", "-hls_list_size", "6", &out]);
        let d = path(&dir);
        let _server = spawn("python3", &["-m", "http.server", "19931", "--bind", "127.0.0.1", "--directory", &d]);
        std::thread::sleep(Duration::from_secs(5));
        rows.push(("hls", measure(json!("http://127.0.0.1:19931/live.m3u8"), 15)));
    }
    print_rows(&rows);
}

fn print_rows(rows: &[(&str, (f64, InputStats))]) {
    println!("\n| Input | CPU, share of one core | kbps | fps | size |\n|---|---|---|---|---|");
    for (name, (cpu, s)) in rows {
        println!("| {name} | {:.1} % | {} | {} | {}x{} |", cpu * 100.0, s.kbps, s.fps, s.width, s.height);
        assert_eq!((s.width, s.height), (1920, 1080), "{name} carried nothing: {s:?}");
    }
}
