//! A core for `collab.rs`: one scene of two named boxes, on a real port.

use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use std::sync::Arc;

/// A core with one scene of two named boxes, listening on a port the
/// operating system picked. Loopback unless `GMX_TEST_BIND` names an address,
/// for a machine whose VPN resets loopback connections.
pub async fn serve() -> String {
    let _ = gstreamer::init();
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    cfg.canvas.width = 320;
    cfg.canvas.height = 180;
    cfg.canvas.fps = 30;
    cfg.multiview.enabled = false;

    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building the mixer");
    mix.start().expect("starting the mixer");
    let multiview = mix.multiview_handle();
    let preview = mix.preview_handle();
    let encoder = mix.encoder_handle();
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));

    let scenes = godwinmix_core::scene::server::SceneServer::in_memory(
        godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
    );
    scenes
        .edit(None, |doc| {
            let mut scene = godwinmix_core::scene::Scene::new("wide");
            for name in ["left", "right"] {
                let mut item = godwinmix_core::scene::Item::new(
                    godwinmix_core::scene::Content::Source { source: format!("cam-{name}") },
                );
                item.name = Some(name.into());
                scene.items.push(item);
            }
            doc.scenes.push(scene);
            Ok(())
        })
        .expect("a scene to edit");

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
    let host = std::env::var("GMX_TEST_BIND").unwrap_or_else(|_| "127.0.0.1".into());
    let listener = tokio::net::TcpListener::bind(format!("{host}:0")).await.expect("a port");
    let address = listener.local_addr().expect("the port it picked");
    tokio::spawn(async move {
        let _ = godwinmix::control::serve_on(listener, app).await;
    });
    format!("ws://{address}/rpc")
}
