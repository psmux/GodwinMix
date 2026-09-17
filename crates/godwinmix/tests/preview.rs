//! The armed scene, over a real `/rpc` socket.
//!
//! The connection used to hold a `PreviewSubscription` and never read it: the
//! `select!` loop polled the mosaic alone, so every JPEG the preview
//! compositor published was dropped and a client that asked for `ext.preview`
//! got a built pipeline and nothing to draw. That is a fact about the loop, so
//! it takes a real server on a real port to prove it fixed; calling a method
//! directly would prove nothing about it.
//!
//! Real GStreamer elements, two test patterns, and a small canvas, as every
//! other test in this repository uses.

use futures_util::{SinkExt, StreamExt};
use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use godwinmix_protocol::rpc::{is_preview_frame, read_frame_header};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// A core with a mosaic and two test patterns. The scene is made over the
/// wire, by `scene.create_from`, so the items are laid out the way an
/// operator's are rather than by hand here.
async fn serve() -> (String, Arc<tokio::sync::Notify>) {
    let _ = gstreamer::init();
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    cfg.canvas.width = 640;
    cfg.canvas.height = 360;
    cfg.canvas.fps = 30;
    cfg.multiview.enabled = true;
    cfg.multiview.width = 640;
    cfg.multiview.height = 360;
    cfg.multiview.fps = 8;
    cfg.sources = ["test://smpte", "test://ball"]
        .iter()
        .zip(["bars", "ball"])
        .map(|(uri, id)| {
            toml::from_str(&format!("id = \"{id}\"\ntype = \"test/source\"\nuri = \"{uri}\"\n"))
                .expect("a valid source document")
        })
        .collect();
    cfg.safety.min_hold_ms = 0;

    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building the mixer");
    mix.start().expect("starting the mixer");
    let multiview = mix.multiview_handle();
    let preview = mix.preview_handle();
    let encoder = mix.encoder_handle();
    // As in `patches.rs`: the mixer thread outlives the test on purpose.
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));

    let quit = Arc::new(tokio::sync::Notify::new());
    let scenes = godwinmix_core::scene::server::SceneServer::in_memory(
        godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
    );

    let app = AppState::new(
        &cfg,
        Engine {
            mixer: handle.clone(),
            multiview,
            preview,
            encoder,
            library: Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone())),
            converter: Arc::new(godwinmix_core::convert::Converter::new(handle.clone(), 1, 5)),
            quit: quit.clone(),
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
    (format!("ws://{address}/rpc"), quit)
}

struct Client {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: u64,
    /// The last `event/multiview.layout` this client was sent.
    layout: Option<Value>,
}

/// One preview frame, header read.
struct Frame {
    seq: u32,
    layout: u32,
    jpeg: Vec<u8>,
}

impl Client {
    async fn open(url: &str) -> Client {
        let (socket, _) = tokio_tungstenite::connect_async(url).await.expect("connecting to /rpc");
        Client { socket, next_id: 1, layout: None }
    }

    async fn call(&mut self, method: &str, params: Value) -> Option<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.socket
            .send(Message::Text(request.to_string().into()))
            .await
            .expect("sending a request");
        for _ in 0..80 {
            let frame =
                tokio::time::timeout(Duration::from_secs(10), self.socket.next()).await.ok()??;
            let Ok(Message::Text(text)) = frame else { continue };
            let value: Value = serde_json::from_str(&text).ok()?;
            self.remember(&value);
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value.get("result").cloned();
            }
        }
        None
    }

    fn remember(&mut self, value: &Value) {
        if value.get("method").and_then(Value::as_str) == Some("event/multiview.layout") {
            self.layout = value.get("params").cloned();
        }
    }

    /// Read binary frames until `want` preview frames have arrived, or time is
    /// up. Text frames on the way are remembered, not discarded.
    async fn preview_frames(&mut self, want: usize, secs: u64) -> Vec<Frame> {
        let mut frames = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
        while frames.len() < want {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                break;
            }
            let Ok(Some(Ok(message))) =
                tokio::time::timeout(left, self.socket.next()).await
            else {
                break;
            };
            match message {
                Message::Text(text) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        self.remember(&value);
                    }
                }
                Message::Binary(bytes) => {
                    let (seq, layout, _) =
                        read_frame_header(&bytes).expect("every binary frame has a header");
                    if is_preview_frame(seq) {
                        frames.push(Frame {
                            seq: godwinmix_protocol::rpc::frame_seq(seq),
                            layout,
                            jpeg: bytes[16..].to_vec(),
                        });
                    }
                }
                _ => {}
            }
        }
        frames
    }
}

/// The whole of the defect, end to end: arm a scene, ask for the preview, and
/// get pictures of it.
#[tokio::test(flavor = "multi_thread")]
async fn an_armed_scene_reaches_a_client_that_asked_for_the_preview() {
    let (url, _quit) = serve().await;
    let mut client = Client::open(&url).await;
    client
        .call("core.subscribe", json!({ "ext": { "preview": { "fps": 8, "width": 640 } } }))
        .await
        .expect("subscribing with ext.preview");

    // Nothing is armed yet, so the layout says so rather than leaving a client
    // waiting for a picture that is not coming.
    let empty = client.preview_frames(4, 30).await;
    assert!(
        !empty.is_empty(),
        "a client that asked for ext.preview was sent no preview frames at all"
    );
    let black = empty.iter().map(|f| f.jpeg.len()).max().unwrap_or(0);
    let layout = client.layout.clone().expect("a preview client is told about the grid");
    assert_eq!(layout["preview_empty"], json!(true), "{layout}");
    assert!(layout["preview_note"].is_string(), "an empty preview says why: {layout}");

    // Arm a two box of both patterns, and the same socket starts carrying it.
    // The scene is made over the wire rather than by hand, because an item
    // built with a default transform has no frame and draws nothing.
    let scene = client
        .call("scene.create_from", json!({ "sources": ["bars", "ball"], "name": "two box" }))
        .await
        .expect("a scene to arm");
    let id = scene["id"].as_str().expect("the new scene's id").to_string();
    client.call("scene.preview.set", json!({ "scene": id })).await.expect("arming");

    // The compositor takes a moment to take its slots and the sources a moment
    // to reach it, so this waits for the picture to arrive rather than
    // asserting on the first frame after the call. The empty preview is the
    // backdrop alone, which encodes to about 4.3 kB at 640 wide against about
    // 8 kB for the two patterns, so half again is a wide margin either side.
    let mut armed = Vec::new();
    for _ in 0..20 {
        let batch = client.preview_frames(4, 30).await;
        assert!(!batch.is_empty(), "the preview stopped once a scene was armed");
        let biggest = batch.iter().map(|f| f.jpeg.len()).max().unwrap_or(0);
        armed = batch;
        if biggest * 2 > black * 3 {
            break;
        }
    }

    for frame in &armed {
        assert_eq!(&frame.jpeg[..2], b"\xff\xd8", "a preview frame is a JPEG");
        assert_eq!(frame.layout, 0, "the preview is one picture, so it names no grid");
    }
    assert!(
        armed.windows(2).all(|w| w[1].seq > w[0].seq),
        "the preview counts its own frames"
    );
    let picture = armed.iter().map(|f| f.jpeg.len()).max().unwrap_or(0);
    assert!(
        picture * 2 > black * 3,
        "the armed preview encoded to {picture} bytes against {black} for an empty one. \
         A picture that small is still the backdrop: the scene never reached it"
    );
    let layout = client.layout.clone().expect("the layout again");
    assert_eq!(layout["preview_empty"], json!(false), "{layout}");
}

/// Nothing runs, and nothing is written, unless somebody asked. A client on
/// the mosaic alone is sent no preview frames while another holds one up.
#[tokio::test(flavor = "multi_thread")]
async fn a_client_that_asked_for_the_mosaic_alone_is_sent_no_preview_frames() {
    let (url, _quit) = serve().await;
    let mut watcher = Client::open(&url).await;
    watcher
        .call("core.subscribe", json!({ "ext": { "preview": { "fps": 8, "width": 640 } } }))
        .await
        .expect("subscribing with ext.preview");
    assert!(
        !watcher.preview_frames(2, 30).await.is_empty(),
        "the preview compositor never produced a frame, so this proves nothing"
    );

    let mut quiet = Client::open(&url).await;
    quiet
        .call("core.subscribe", json!({ "ext": { "multiview": { "fps": 8, "width": 640 } } }))
        .await
        .expect("subscribing to the mosaic alone");
    let heard = quiet.preview_frames(1, 3).await;
    assert!(heard.is_empty(), "a client that asked for the mosaic was sent preview frames");
}
