//! RTSP, HLS, DASH and RTMP, each pulled from a server run by the test.

use gstreamer_rtsp_server as rtsp;
use gstreamer_rtsp_server::prelude::*;
use serde_json::json;

use super::*;

/// An RTSP server on `port` with one mount, `/cam`, H.264 and AAC, on a
/// main loop of its own, like a camera.
struct Camera {
    main_loop: glib::MainLoop,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Camera {
    fn new(port: u16) -> Camera {
        gmx_netkit::init().unwrap();
        let ctx = glib::MainContext::new();
        let server = rtsp::RTSPServer::new();
        server.set_address("127.0.0.1");
        server.set_service(&port.to_string());
        let factory = rtsp::RTSPMediaFactory::new();
        factory.set_launch(
            "( videotestsrc is-live=true ! video/x-raw,width=320,height=240,framerate=25/1 ! x264enc tune=zerolatency key-int-max=25 \
             ! rtph264pay name=pay0 pt=96 audiotestsrc is-live=true ! avenc_aac ! rtpmp4gpay name=pay1 pt=97 )",
        );
        factory.set_shared(true);
        server.mount_points().unwrap().add_factory("/cam", factory);
        let id = server.attach(Some(&ctx)).expect("the RTSP server binds");
        let main_loop = glib::MainLoop::new(Some(&ctx), false);
        let ml = main_loop.clone();
        // The source lives on `ctx`, which goes with the loop; `SourceId::remove`
        // would look for it on the default context and complain.
        let _ = id;
        let thread = std::thread::spawn(move || ml.run());
        Camera { main_loop, thread: Some(thread) }
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        self.main_loop.quit();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[test]
fn rtsp_pulls_a_camera_over_tcp_and_over_udp() {
    let _one = one_at_a_time();
    for (transport, port) in [("tcp", 19930), ("udp", 19938)] {
        let _cam = Camera::new(port);
        let rx = start(json!({"uri": format!("rtsp://127.0.0.1:{port}/cam"), "params": {"transport": transport}}), &Context::default());
        assert!(eventually(15, || rx.got.keyframes() >= 3 && rx.got.frames(TagKind::Audio) > 20), "{transport}: {:?}", rx.got.last());
        let s = rx.got.last();
        assert_eq!((s.video_codec.as_str(), s.width, s.height, s.audio_codec.as_str()), ("h264", 320, 240, "aac"), "{transport}: {s:?}");
    }
}

/// Serve `dir` over HTTP on `port` with Python's server, as a CDN would.
fn serve(dir: &Path, port: u16) -> Sender {
    let d = path(dir);
    spawn("python3", &["-m", "http.server", &port.to_string(), "--bind", "127.0.0.1", "--directory", &d])
}

#[test]
fn hls_and_dash_are_pulled_and_paced() {
    let _one = one_at_a_time();
    if !which("ffmpeg") || !which("python3") {
        return;
    }
    for (kind, port, file) in [("hls", 19932u16, "live.m3u8"), ("dash", 19933, "live.mpd")] {
        let dir = scratch().join(kind);
        std::fs::create_dir_all(&dir).unwrap();
        let out = path(&dir.join(file));
        // Paced by the `realtime` filters, not by `-re` alone: the ffmpeg
        // Homebrew ships did not pace its lavfi inputs with `-re`, wrote the
        // thirty seconds in about six, segment 24 five seconds in, and ended
        // the playlist before a live player had joined.
        let mut args = vec!["-loglevel", "error", "-re", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25,realtime", "-f", "lavfi", "-i", "sine,arealtime",
            "-t", "30", "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-c:a", "aac"];
        args.extend(if kind == "hls" { ["-f", "hls", "-hls_time", "1", "-hls_list_size", "6"] } else { ["-f", "dash", "-seg_duration", "1", "-window_size", "6"] });
        args.push(&out);
        let _encoder = spawn("ffmpeg", &args);
        let _server = serve(&dir, port);
        assert!(eventually(10, || dir.join(file).is_file()), "{kind}: ffmpeg wrote no {file}");
        std::thread::sleep(Duration::from_secs(3));
        let rx = start(json!(format!("http://127.0.0.1:{port}/{file}")), &Context::default());
        let arrived = eventually(20, || rx.got.keyframes() >= 4);
        // Which adaptive demuxers this GStreamer has: on the macOS runner DASH
        // stayed connecting with no error, and the log should say what it ran.
        let have: Vec<&str> = ["dashdemux2", "dashdemux", "hlsdemux2", "hlsdemux", "souphttpsrc", "curlhttpsrc"]
            .into_iter()
            .filter(|e| gstreamer::ElementFactory::find(e).is_some())
            .collect();
        let manifest = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
        assert!(arrived, "{kind}: {:?}; elements here {have:?}; the manifest now: {manifest}", rx.got.last());
        let s = rx.got.last();
        assert_eq!((s.video_codec.as_str(), s.width, s.audio_codec.as_str()), ("h264", 320, "aac"), "{kind}: {s:?}");
    }
}

/// ffmpeg as an RTMP server that a player calls (`-listen 1`), which is what
/// someone else's RTMP server is to a direct show.
#[test]
fn rtmp_is_played_from_someone_elses_server() {
    let _one = one_at_a_time();
    if !which("ffmpeg") {
        return;
    }
    let _server = spawn("ffmpeg", &["-loglevel", "error", "-re", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-f", "lavfi", "-i", "sine",
        "-t", "30", "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-c:a", "aac", "-f", "flv", "-listen", "1", "rtmp://127.0.0.1:19934/live/feed"]);
    std::thread::sleep(Duration::from_millis(800));
    let rx = start(json!("rtmp://127.0.0.1:19934/live/feed"), &Context::default());
    assert!(eventually(15, || rx.got.keyframes() >= 3), "{:?}", rx.got.last());
    let s = rx.got.last();
    assert_eq!((s.video_codec.as_str(), s.width, s.audio_codec.as_str()), ("h264", 320, "aac"), "{s:?}");
}
