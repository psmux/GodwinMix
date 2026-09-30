//! `/hls/*` over a real port: a core with a control token, one `hls/output`
//! packaging the programme as LL-HLS, and plain HTTP against it.
//!
//! What only a real server proves: that a viewer key opens the playlists and
//! nothing else does, that a blocking reload holds and then answers, that the
//! cache headers are right, and that a viewer who stops reading a segment
//! holds nobody else up.

use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

const TOKEN: &str = "hls-test-control-token";

async fn serve() -> String {
    let _ = gstreamer::init();
    let text = format!(
        r#"
        [canvas]
        width = 640
        height = 360
        fps = 30
        [program]
        keyframe_interval_secs = 1
        video_bitrate_kbps = 1500
        [control]
        token = "{TOKEN}"
        [[sources]]
        id = "ball"
        type = "test/source"
        uri = "test://ball"
        [[outputs]]
        id = "viewers"
        type = "hls/output"
        uri = "hls://viewers"
        params = {{ segment_ms = 1000, part_ms = 250, window = 6 }}
        "#
    );
    let cfg: Config = toml::from_str(&text).expect("a valid config");
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building the mixer");
    mix.start().expect("starting the mixer");
    let (multiview, preview, encoder) = (mix.multiview_handle(), mix.preview_handle(), mix.encoder_handle());
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));
    let scenes = godwinmix_core::scene::server::SceneServer::in_memory(godwinmix_core::caps::CanvasCaps::new(&cfg.canvas));
    let app = AppState::new(
        &cfg,
        Engine {
            mixer: handle.clone(),
            multiview,
            preview,
            encoder,
            library: Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone())),
            converter: Arc::new(godwinmix_core::convert::Converter::new(handle.clone(), 1, 5)),
            quit: Arc::new(tokio::sync::Notify::new()),
            scenes,
            plugins: godwinmix_core::plugin::supervisor::Supervisor::detached(),
        },
        false,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("the port it picked");
    tokio::spawn(async move {
        let _ = godwinmix::control::serve_on(listener, app).await;
    });
    format!("http://{address}")
}

async fn output(base: &str) -> Value {
    reqwest::Client::new()
        .get(format!("{base}/api/v1/outputs/viewers"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .expect("output.get")
        .json()
        .await
        .expect("json")
}

async fn get(url: &str) -> reqwest::Response {
    reqwest::get(url).await.expect("a response")
}

/// The media playlist's hinted part, `(msn, part)`.
fn hint(text: &str) -> (u64, u32) {
    let at = text.find("#EXT-X-PRELOAD-HINT:TYPE=PART,URI=\"").expect("a preload hint") + 35;
    let uri = &text[at..at + text[at..].find(".m4s").unwrap()];
    let (m, p) = uri.split_once('.').unwrap();
    (m.parse().unwrap(), p.parse().unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_viewer_key_plays_ll_hls_and_nothing_else_gets_in() {
    let base = serve().await;
    let status = output(&base).await;
    let path = status["playback"]["master_url_path"].as_str().expect("playback.master_url_path").to_string();
    let key = path.split("key=").nth(1).unwrap().to_string();

    // No key, no token: refused, and told where the link is.
    let refused = get(&format!("{base}/hls/viewers/master.m3u8")).await;
    assert_eq!(refused.status(), 401);
    let body: Value = refused.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("playback.master_url_path"), "{body}");
    // The viewer key opens nothing else on the port.
    let api = get(&format!("{base}/api/v1/outputs?key={key}")).await;
    assert_eq!(api.status(), 401, "a viewer key is not a control token");
    let wrong = get(&format!("{base}/hls/viewers/master.m3u8?key=not-the-key-at-all-xx")).await;
    assert_eq!(wrong.status(), 401);

    // The master waits for the first segments, then names the rung with the
    // key and a viewer id on every URI.
    let master = get(&format!("{base}{path}")).await;
    assert_eq!(master.status(), 200);
    assert_eq!(master.headers()["content-type"], "application/vnd.apple.mpegurl");
    assert_eq!(master.headers()["cache-control"], "no-store");
    let text = master.text().await.unwrap();
    assert!(text.contains("#EXT-X-STREAM-INF:BANDWIDTH="), "{text}");
    assert!(text.contains("RESOLUTION=640x360") && text.contains("avc1.") && text.contains("mp4a.40.2"), "{text}");
    let rung_line = text.lines().find(|l| l.starts_with("programme/index.m3u8")).expect("the programme rung");
    assert!(rung_line.contains(&format!("key={key}")) && rung_line.contains("&v="), "{rung_line}");
    let query = rung_line.split_once('?').unwrap().1.to_string();

    // A token with read gets in too, with no key.
    let by_token = reqwest::Client::new().get(format!("{base}/hls/viewers/master.m3u8")).bearer_auth(TOKEN).send().await.unwrap();
    assert_eq!(by_token.status(), 200);

    let media_url = format!("{base}/hls/viewers/programme/index.m3u8?{query}");
    let media = get(&media_url).await.text().await.unwrap();
    // 250 ms at 30 fps is parts of eight frames, 267 ms, and the target says so.
    assert!(media.contains("#EXT-X-PART-INF:PART-TARGET=0.2"), "{media}");
    assert!(media.contains("CAN-BLOCK-RELOAD=YES"), "{media}");
    assert!(media.contains(&format!("init.mp4?{query}")), "{media}");

    // A blocking reload for the hinted part holds until it exists.
    let (m, p) = hint(&media);
    let started = Instant::now();
    let blocked = get(&format!("{media_url}&_HLS_msn={m}&_HLS_part={p}")).await;
    assert_eq!(blocked.status(), 200);
    let waited = started.elapsed();
    let after = blocked.text().await.unwrap();
    // Listed as a part, not only hinted again.
    let listed = after.lines().any(|l| l.starts_with("#EXT-X-PART:") && l.contains(&format!("URI=\"{m}.{p}.m4s?")));
    assert!(listed, "the part it waited for is listed: {after}");
    assert!(waited < Duration::from_millis(1500), "held {waited:?} for one part");

    // Too far ahead is a 400 that says why.
    let ahead = get(&format!("{media_url}&_HLS_msn={}", m + 10)).await;
    assert_eq!(ahead.status(), 400);

    // The hinted part itself, asked for before it exists, is held and served.
    let (m2, p2) = hint(&after);
    let part = get(&format!("{base}/hls/viewers/programme/{m2}.{p2}.m4s?{query}")).await;
    assert_eq!(part.status(), 200);
    assert_eq!(part.headers()["content-type"], "video/mp4");
    assert!(part.headers()["cache-control"].to_str().unwrap().contains("immutable"));
    let bytes = part.bytes().await.unwrap();
    assert_eq!(&bytes[4..8], b"moof");

    // A whole segment, and the init.
    let whole = after.lines().rev().find(|l| !l.starts_with('#') && l.contains(".m4s")).expect("a segment").to_string();
    let seg = get(&format!("{base}/hls/viewers/programme/{whole}")).await;
    assert_eq!(seg.status(), 200);
    let len: usize = seg.headers()["content-length"].to_str().unwrap().parse().unwrap();
    assert_eq!(seg.bytes().await.unwrap().len(), len);
    let init = get(&format!("{base}/hls/viewers/programme/init.mp4?{query}")).await.bytes().await.unwrap();
    assert_eq!(&init[4..8], b"ftyp");

    // The same segments as DASH, with the key on every URL.
    let mpd = get(&format!("{base}/hls/viewers/manifest.mpd?key={key}")).await;
    assert_eq!(mpd.status(), 200);
    assert_eq!(mpd.headers()["content-type"], "application/dash+xml");
    let mpd = mpd.text().await.unwrap();
    assert!(mpd.contains("type=\"dynamic\"") && mpd.contains("<Representation id=\"programme\""), "{mpd}");
    assert!(mpd.contains(&format!("media=\"programme/$Number$.m4s?key={key}&amp;v=")), "{mpd}");
    let start: u64 = mpd.split("startNumber=\"").nth(1).unwrap().split('"').next().unwrap().parse().unwrap();
    let listed = get(&format!("{base}/hls/viewers/programme/{start}.m4s?key={key}")).await;
    assert_eq!(listed.status(), 200, "the MPD's first segment is there");

    // Wrong names are 404s that say what there is.
    let rung = get(&format!("{base}/hls/viewers/4k/index.m3u8?{query}")).await;
    assert_eq!(rung.status(), 404);
    assert!(rung.text().await.unwrap().contains("programme"));
    let gone = get(&format!("{base}/hls/viewers/programme/{}.m4s?{query}", m + 50)).await;
    assert_eq!(gone.status(), 404);
    let none = get(&format!("{base}/hls/nobody/master.m3u8")).await;
    assert!(none.text().await.unwrap().contains("viewers"));

    // The fetches above were one viewer, and egress is visible.
    tokio::time::sleep(Duration::from_millis(2200)).await;
    let status = output(&base).await;
    assert!(status["viewers"].as_u64().unwrap() >= 1, "{status}");
    assert!(status["playback"]["viewers"].as_u64().unwrap() >= 1, "{status}");
    assert_eq!(status["type"], "hls/output");

    stalled_reader_holds_nobody_up(&base, &query, &media_url).await;
}

/// Ask for a segment and never read it, then show every other request is
/// still answered at once.
async fn stalled_reader_holds_nobody_up(base: &str, query: &str, media_url: &str) {
    let address = base.trim_start_matches("http://");
    let text = get(media_url).await.text().await.unwrap();
    let seg = text.lines().rev().find(|l| !l.starts_with('#') && l.contains(".m4s")).unwrap().to_string();
    let mut stalled = Vec::new();
    for _ in 0..8 {
        let mut s = tokio::net::TcpStream::connect(address).await.unwrap();
        let req = format!("GET /hls/viewers/programme/{seg} HTTP/1.1\r\nhost: {address}\r\n\r\n");
        s.write_all(req.as_bytes()).await.unwrap();
        stalled.push(s);
    }
    let started = Instant::now();
    for _ in 0..6 {
        let text = get(media_url).await.text().await.unwrap();
        let (m, p) = hint(&text);
        let r = get(&format!("{media_url}&_HLS_msn={m}&_HLS_part={p}")).await;
        assert_eq!(r.status(), 200);
    }
    let took = started.elapsed();
    assert!(took < Duration::from_secs(4), "six parts took {took:?} with stalled readers open");
    let init = get(&format!("{base}/hls/viewers/programme/init.mp4?{query}")).await;
    assert_eq!(init.status(), 200);
    drop(stalled);
}
