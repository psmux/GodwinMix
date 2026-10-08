//! What a clip does at its end, through the public protocol: `at_end` set
//! with `source.set`, read back in the status, and a clip set to leave the
//! scene taking the programme somewhere else when it ends on air, with Studio
//! mode's armed scene and without it.
//!
//! A real server on a real port, real GStreamer, a two second clip written
//! for the test and a test pattern.

use futures_util::{SinkExt, StreamExt};
use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// A two second clip with picture and sound, from elements every GStreamer
/// install has.
fn write_clip(name: &str) -> Option<String> {
    let _ = gstreamer::init();
    let dir = std::env::temp_dir().join(format!("gmx-leave-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("intro.avi").display().to_string().replace('\\', "/");
    let desc = format!(
        "videotestsrc num-buffers=50 pattern=ball ! video/x-raw,width=320,height=180,framerate=25/1 ! jpegenc ! \
         avimux name=m ! filesink location=\"{path}\" \
         audiotestsrc num-buffers=86 samplesperbuffer=1024 ! audio/x-raw,format=S16LE,rate=44100,channels=2 ! m."
    );
    use gstreamer::prelude::*;
    let pipeline = gstreamer::parse::launch(&desc).ok()?;
    pipeline.set_state(gstreamer::State::Playing).ok()?;
    let done = pipeline.bus()?.timed_pop_filtered(
        gstreamer::ClockTime::from_seconds(20),
        &[gstreamer::MessageType::Eos, gstreamer::MessageType::Error],
    );
    let _ = pipeline.set_state(gstreamer::State::Null);
    done.filter(|m| m.type_() == gstreamer::MessageType::Eos).map(|_| path)
}

async fn serve() -> String {
    let _ = gstreamer::init();
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    cfg.canvas.width = 320;
    cfg.canvas.height = 180;
    cfg.canvas.fps = 15;
    cfg.sources = vec![toml::from_str("id = \"cam\"\ntype = \"test/source\"\nuri = \"test://smpte\"\n").unwrap()];
    cfg.safety.min_hold_ms = 0;
    cfg.safety.flash_guard = false;

    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building the mixer");
    mix.start().expect("starting the mixer");
    let multiview = mix.multiview_handle();
    let preview = mix.preview_handle();
    let encoder = mix.encoder_handle();
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
    format!("ws://{address}/rpc")
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct Client {
    socket: Socket,
    next_id: u64,
}

impl Client {
    async fn open(url: &str) -> Client {
        let (socket, _) = tokio_tungstenite::connect_async(url).await.expect("connecting to /rpc");
        Client { socket, next_id: 1 }
    }

    async fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.socket.send(Message::Text(request.to_string().into())).await.expect("sending");
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(10), self.socket.next())
                .await
                .expect("an answer in time")
                .expect("an open socket")
                .expect("a frame");
            let Message::Text(text) = frame else { continue };
            let value: Value = serde_json::from_str(&text).expect("JSON");
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value.get("result").cloned().unwrap_or_else(|| panic!("{method} failed: {value}"));
            }
        }
    }

    /// The scene on air once `want` holds, or what was on air when time ran out.
    async fn on_air_when(&mut self, secs: u64, want: impl Fn(&Value) -> bool) -> Value {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
        loop {
            let now = self.call("program.get", json!({})).await;
            if want(&now) || tokio::time::Instant::now() > deadline {
                return now;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    async fn clip_row(&mut self) -> Value {
        let status = self.call("source.list", json!({})).await;
        let list = status.get("sources").cloned().unwrap_or(status);
        list.as_array().and_then(|l| l.iter().find(|s| s["id"] == "intro").cloned()).expect("the clip is listed")
    }
}

async fn with_clip(name: &str) -> Option<Client> {
    let clip = write_clip(name)?;
    let url = serve().await;
    let mut client = Client::open(&url).await;
    client.call("source.add", json!({ "id": "intro", "uri": clip, "params": { "at_end": "leave" } })).await;
    client.call("scene.create_from", json!({ "sources": ["cam"], "name": "Studio" })).await;
    client.call("scene.create_from", json!({ "sources": ["intro"], "name": "Opener" })).await;
    Some(client)
}

/// Outside Studio mode the clip's scene gives way to what was on before it.
#[tokio::test(flavor = "multi_thread")]
async fn a_clip_set_to_leave_goes_back_to_the_scene_before_it() {
    let Some(mut client) = with_clip("before").await else { return println!("skipping: no test clip") };
    client.call("program.take", json!({ "scene": "Studio" })).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    client.call("program.take", json!({ "scene": "Opener" })).await;
    let now = client.on_air_when(10, |p| p["scene"] == "Studio").await;
    assert_eq!(now["scene"], json!("Studio"), "the programme did not leave the clip: {now}");
    let row = client.clip_row().await;
    assert_eq!(row["at_end"], json!("leave"), "{row}");
    assert_eq!(row["ended"], json!(true), "{row}");
}

/// In Studio mode the scene armed in Preview is what comes next.
#[tokio::test(flavor = "multi_thread")]
async fn a_clip_set_to_leave_takes_the_armed_scene() {
    let Some(mut client) = with_clip("armed").await else { return println!("skipping: no test clip") };
    client.call("scene.create_from", json!({ "sources": ["cam"], "name": "Wide" })).await;
    client.call("program.take", json!({ "scene": "Studio" })).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    client.call("program.take", json!({ "scene": "Opener" })).await;
    client.call("scene.preview.set", json!({ "scene": "Wide" })).await;
    let now = client.on_air_when(10, |p| p["scene"] == "Wide").await;
    assert_eq!(now["scene"], json!("Wide"), "the armed scene was not taken: {now}");
}

/// Set to hold, through `source.set`, the clip stays on air on its last
/// frame, and Repeat set the same way is read back.
#[tokio::test(flavor = "multi_thread")]
async fn a_clip_set_to_hold_stays_on_air() {
    let Some(mut client) = with_clip("hold").await else { return println!("skipping: no test clip") };
    client.call("source.set", json!({ "id": "intro", "params": { "at_end": "hold" } })).await;
    client.call("program.take", json!({ "scene": "Studio" })).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    client.call("program.take", json!({ "scene": "Opener" })).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let now = client.call("program.get", json!({})).await;
    assert_eq!(now["scene"], json!("Opener"), "a held clip left the scene: {now}");
    let row = client.clip_row().await;
    assert_eq!((row["at_end"].clone(), row["ended"].clone(), row["state"].clone()), (json!("hold"), json!(true), json!("live")), "{row}");

    client.call("source.set", json!({ "id": "intro", "params": { "at_end": "repeat" } })).await;
    let row = client.clip_row().await;
    assert_eq!(row["at_end"], json!("repeat"), "{row}");
}
