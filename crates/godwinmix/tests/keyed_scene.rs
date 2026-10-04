//! A presenter keyed into a designed studio with `scene.create_from`, and
//! `source.key_color`, over `/rpc`, the way an agent
//! reaches them: a real server, real GStreamer, a test pattern for a camera
//! and two pictures in the media library.

use futures_util::{SinkExt, StreamExt};
use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn serve(media: &std::path::Path) -> String {
    let _ = gstreamer::init();
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    (cfg.canvas.width, cfg.canvas.height, cfg.canvas.fps) = (320, 180, 15);
    cfg.sources = vec![toml::from_str("id = \"cam\"\ntype = \"test/source\"\nuri = \"test://smpte\"\n").unwrap()];
    cfg.media.dir = media.to_string_lossy().into_owned();
    cfg.safety.min_hold_ms = 0;
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building the mixer");
    mix.start().expect("starting the mixer");
    let (multiview, preview, encoder) = (mix.multiview_handle(), mix.preview_handle(), mix.encoder_handle());
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));
    let scenes = godwinmix_core::scene::server::SceneServer::in_memory(godwinmix_core::caps::CanvasCaps::new(&cfg.canvas));
    let engine = Engine {
        mixer: handle.clone(),
        multiview,
        preview,
        encoder,
        library: Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone())),
        converter: Arc::new(godwinmix_core::convert::Converter::new(handle.clone(), 1, 5)),
        quit: Arc::new(tokio::sync::Notify::new()),
        scenes,
        plugins: godwinmix_core::plugin::supervisor::Supervisor::detached(),
    };
    let app = AppState::new(&cfg, engine, false);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("the port it picked");
    tokio::spawn(async move {
        let _ = godwinmix::control::serve_on(listener, app).await;
    });
    format!("ws://{address}/rpc")
}

async fn ask(socket: &mut Socket, id: u64, method: &str, params: Value) -> Value {
    let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    socket.send(Message::Text(request.to_string().into())).await.expect("sending");
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(15), socket.next()).await.expect("an answer in time");
        let Some(Ok(Message::Text(text))) = frame else { continue };
        let value: Value = serde_json::from_str(&text).expect("JSON");
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            return value;
        }
    }
}

/// A PNG of one colour, written by GStreamer.
fn png(path: &std::path::Path, rgba: [u8; 4]) {
    use gstreamer::prelude::*;
    let _ = gstreamer::init();
    let pipe = gstreamer::parse::launch(&format!(
        "appsrc name=s caps=video/x-raw,format=RGBA,width=32,height=18,framerate=1/1 ! pngenc ! filesink location=\"{}\"",
        path.display().to_string().replace('\\', "/")
    ))
    .unwrap()
    .downcast::<gstreamer::Pipeline>()
    .unwrap();
    let src = pipe.by_name("s").unwrap().downcast::<gstreamer_app::AppSrc>().unwrap();
    pipe.set_state(gstreamer::State::Playing).unwrap();
    src.push_buffer(gstreamer::Buffer::from_mut_slice(rgba.repeat(32 * 18))).unwrap();
    src.end_of_stream().unwrap();
    pipe.bus().unwrap().timed_pop_filtered(gstreamer::ClockTime::from_seconds(5), &[gstreamer::MessageType::Eos]);
    pipe.set_state(gstreamer::State::Null).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_presenter_scene_is_made_in_one_call_from_a_camera_and_two_library_pictures() {
    let media = std::env::temp_dir().join(format!("gmx-virtual-set-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&media);
    std::fs::create_dir_all(&media).unwrap();
    png(&media.join("newsroom.png"), [30, 60, 200, 255]);
    png(&media.join("desk.png"), [200, 30, 30, 128]);
    let url = serve(&media).await;
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.expect("connecting to /rpc");

    let made = ask(&mut socket, 1, "scene.create_from", json!({
        "sources": ["newsroom.png", "cam", "desk.png"], "layout": "virtual-set", "settings": {"key": "#00ff00"}
    }))
    .await;
    let made = made.get("result").unwrap_or_else(|| panic!("scene.create_from failed: {made}"));
    assert_eq!(made["key"], "#00ff00");
    assert_eq!(made["key_from"], "given");
    assert_eq!(made["added"], json!(["newsroom", "desk"]), "both pictures became sources");
    let paths: Vec<&str> = made["geometry"].as_array().unwrap().iter().filter_map(|g| g["path"].as_str()).collect();
    assert_eq!(paths, ["set", "presenter", "foreground"], "back to front, with no empty lower third");
    let presenter = made["geometry"].as_array().unwrap().iter().find(|g| g["path"] == "presenter").unwrap();
    assert_eq!(presenter["y"].as_f64().unwrap() + presenter["height"].as_f64().unwrap(), 180.0, "standing on the bottom edge");

    let again = ask(&mut socket, 2, "scene.create_from", json!({"sources": ["newsroom.png", "cam"], "layout": "virtual-set"})).await;
    assert!(again["result"].get("added").is_none(), "a picture already on the desk is not added twice: {again}");
    assert!(again["result"]["key_from"].is_string(), "a keyed layout always says where its key came from: {again}");

    let refused = ask(&mut socket, 3, "scene.create_from", json!({"sources": ["nowhere.png", "cam"], "layout": "virtual-set"})).await;
    assert_eq!(refused["error"]["data"]["field"], "sources[0]", "{refused}");
    assert!(refused["error"]["data"]["next"].as_str().unwrap().contains("media.upload"), "{refused}");

    // A plain layout has no key, and says nothing about one.
    let plain = ask(&mut socket, 5, "scene.create_from", json!({"sources": ["cam"]})).await;
    assert!(plain["result"].get("key").is_none(), "{plain}");
    let wrong = ask(&mut socket, 6, "scene.create_from", json!({"sources": ["cam"], "layout": "full", "settings": {"key": "auto"}})).await;
    assert!(wrong["error"]["message"].as_str().unwrap_or_default().contains("no setting"), "{wrong}");

    // The green bar of the bars, a little under halfway across.
    let point = ask(&mut socket, 4, "source.key_color", json!({"id": "cam", "x": 0.5, "y": 0.3})).await;
    let color = point["result"]["color"].as_str().unwrap_or_else(|| panic!("{point}"));
    let rgb = godwinmix_core::plugin::filters::chroma::parse_hex(color).unwrap();
    assert!(rgb[1] > 150 && rgb[0] < 90 && rgb[2] < 90, "the colour at the green bar is green: {color}");
    let _ = std::fs::remove_dir_all(&media);
}
