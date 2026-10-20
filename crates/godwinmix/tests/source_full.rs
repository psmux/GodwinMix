//! `/mjpeg/{source}?full=1`: one source at the canvas size, over a real HTTP
//! server on a real port.
//!
//! Interact showed a web page as its multiview cell, a fraction of a 1920
//! pixel sheet, and the page could not be read (tester issue 24). This is the
//! picture it shows now: the source's own frames, whole, made only while a
//! client holds the stream, and with the multiview switched off, which proves
//! the mosaic is not where they come from.

use futures_util::StreamExt;
use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use godwinmix_core::preview::PreviewHandle;
use std::sync::Arc;
use std::time::Duration;

/// A core with one test pattern on a 640 by 360 canvas and no multiview.
async fn serve() -> (String, PreviewHandle, mixer::MixerHandle) {
    let _ = gstreamer::init();
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    cfg.canvas.width = 640;
    cfg.canvas.height = 360;
    cfg.canvas.fps = 30;
    cfg.multiview.enabled = false;
    cfg.sources = vec![toml::from_str("id = \"bars\"\ntype = \"test/source\"\nuri = \"test://smpte\"\n")
        .expect("a valid source document")];

    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building the mixer");
    mix.start().expect("starting the mixer");
    let multiview = mix.multiview_handle();
    let preview = mix.preview_handle();
    let encoder = mix.encoder_handle();
    // As in `preview.rs`: the mixer thread outlives the test on purpose.
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));

    let scenes = godwinmix_core::scene::server::SceneServer::in_memory(
        godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
    );
    let app = AppState::new(
        &cfg,
        Engine {
            mixer: handle.clone(),
            multiview,
            preview: preview.clone(),
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
    (format!("http://{address}"), preview, handle)
}

/// The JPEGs in `bytes`, by their `Content-Length` headers.
fn parts(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while let Some(at) = find(rest, b"Content-Length: ") {
        let after = &rest[at + 16..];
        let Some(end) = find(after, b"\r\n\r\n") else { break };
        let Ok(len) = std::str::from_utf8(&after[..end]).unwrap_or("").trim().parse::<usize>() else { break };
        let body = &after[end + 4..];
        if body.len() < len {
            break;
        }
        out.push(body[..len].to_vec());
        rest = &body[len..];
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Read the stream for `secs`, then hang up.
async fn read_for(url: &str, secs: u64) -> Vec<Vec<u8>> {
    let response = reqwest::get(url).await.expect("the stream answers");
    assert!(response.status().is_success(), "{}: {:?}", response.status(), response.text().await);
    let mut body = response.bytes_stream();
    let mut bytes = Vec::new();
    let until = tokio::time::Instant::now() + Duration::from_secs(secs);
    while let Ok(Some(Ok(chunk))) = tokio::time::timeout_at(until, body.next()).await {
        bytes.extend_from_slice(&chunk);
    }
    parts(&bytes)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_source_at_full_size_is_the_whole_canvas_and_stops_when_the_client_goes() {
    let (base, preview, _mixer) = serve().await;
    // The source has to be up before its tee carries anything.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(preview.clients().count("source"), 0, "nothing is made before anybody asks");

    let url = format!("{base}/mjpeg/bars?full=1");
    let watcher = {
        let preview = preview.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            preview.clients().count("source")
        })
    };
    let frames = read_for(&url, 3).await;
    assert_eq!(watcher.await.unwrap(), 1, "one branch on the source's tee while the stream is open");
    let img = godwinmix_core::snapshot::decode_jpeg(frames.first().expect("no frames came")).expect("a JPEG");
    assert_eq!((img.width(), img.height()), (640, 360), "the canvas size, not a cell of a mosaic");
    // Three seconds at 30 fps, less the start.
    assert!(frames.len() >= 45, "only {} frames in three seconds", frames.len());

    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(preview.clients().count("source"), 0, "the branch went with the client");

    let small = read_for(&format!("{url}&width=320&fps=10"), 2).await;
    let img = godwinmix_core::snapshot::decode_jpeg(small.first().expect("no frames came")).expect("a JPEG");
    assert_eq!((img.width(), img.height()), (320, 180), "width caps it at the canvas's shape");
    assert!(small.len() <= 25, "{} frames in two seconds at fps=10", small.len());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_source_that_is_not_here_is_refused_with_the_ones_that_are() {
    let (base, _preview, _mixer) = serve().await;
    let response = reqwest::get(format!("{base}/mjpeg/nope?full=1")).await.expect("an answer");
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    let text = response.text().await.unwrap();
    assert!(text.contains("nope") && text.contains("bars"), "{text}");
}

/// A source rebuilt under a viewer (a new address, a restart after a stall)
/// gets the branch again on its new tee, and the stream carries on.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_restarted_under_a_viewer_keeps_its_stream() {
    let (base, _preview, mixer) = serve().await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let restart = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        mixer.send(mixer::Command::RestartSource("bars".into())).expect("the restart is sent");
    });
    let frames = read_for(&format!("{base}/mjpeg/bars?full=1"), 8).await;
    restart.await.unwrap();
    // Two seconds before the restart is at most about 60 frames. A branch
    // left on the old tee would stop there, and the stream would end.
    assert!(frames.len() >= 120, "only {} frames in eight seconds across a restart", frames.len());
}
