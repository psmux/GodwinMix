//! Folders and tags in the media library through the method table, on a
//! running core: files and folders move on disk, and every source, template
//! and tag that names them follows in the same call.

use godwinmix::control::{call, methods, AppState};
use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::{self, Mixer};
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::Token;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

#[path = "live/harness.rs"]
mod harness;
use harness::*;

/// A source that draws colour bars but names a library file as its address,
/// which is all a move looks at.
async fn source_naming(core: &Core, id: &str, uri: &str) {
    let source: SourceConfig = toml::from_str(&format!("id = \"{id}\"\ntype = \"test/source\"\nuri = '{uri}'\n")).unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    core.app.mixer.send(mixer::Command::AddSource(Box::new(source), Some(tx))).unwrap();
    rx.await.unwrap().unwrap();
}

async fn uri_of(core: &Core, id: &str) -> String {
    let configs = core.app.mixer.configs().await.unwrap();
    configs.sources.iter().find(|s| s.id == id).map(|s| s.uri.clone()).unwrap_or_default()
}

fn names(listing: &Value) -> Vec<String> {
    listing["items"].as_array().unwrap().iter().map(|i| i["name"].as_str().unwrap().to_string()).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn files_and_folders_move_on_disk_and_what_names_them_follows() {
    if gstreamer::init().is_err() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("gmx-media-library-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["a.png", "b.png", "intro.mp4"] {
        std::fs::write(dir.join(f), b"x").unwrap();
    }
    std::fs::write(dir.join("card.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="Sermons/b.png"/></svg>"#).unwrap();
    let media = dir.display().to_string();
    let core = Core::start_configured(Default::default(), false, |cfg| cfg.media.dir = media.clone()).await;
    let token = desk();

    // Folders are directories, made at any depth.
    let made = core.call(&token, "media.folder.add", json!({ "name": "Sermons/2026" })).await.unwrap();
    assert_eq!(made["path"], "Sermons/2026");
    assert!(dir.join("Sermons/2026").is_dir());

    // A source on programme refuses the move of its file unless forced.
    let a = dir.join("a.png").display().to_string();
    source_naming(&core, "pic", &a).await;
    core.wait_live("pic").await;
    core.call(&token, "program.take", json!({ "source": "pic" })).await.unwrap();
    let refused = core.call(&token, "media.move", json!({ "name": "a.png", "folder": "Sermons" })).await.unwrap_err();
    assert_eq!(refused.data["sources"], json!(["pic"]), "{refused:?}");
    assert!(refused.message.contains("force"), "names the next step: {}", refused.message);
    assert!(dir.join("a.png").is_file(), "nothing moved");
    let moved = core.call(&token, "media.move", json!({ "name": "a.png", "folder": "Sermons", "force": true })).await.unwrap();
    assert_eq!(moved["moved"], json!([{ "from": "a.png", "to": "Sermons/a.png" }]));
    assert_eq!(moved["updated"], json!(["source pic"]), "taken out before the move and put back after: {moved}");
    core.wait_live("pic").await;
    assert!(uri_of(&core, "pic").await.ends_with("a.png") && uri_of(&core, "pic").await.contains("Sermons"), "{}", uri_of(&core, "pic").await);

    // A source off air by file URI, and a template naming a picture, follow
    // a whole folder's move.
    core.call(&token, "media.move", json!({ "name": "b.png", "folder": "Sermons" })).await.unwrap();
    source_naming(&core, "pic2", &godwinmix_core::input::file_uri(&dir.join("Sermons/b.png"))).await;
    core.call(&token, "media.tag", json!({ "names": ["Sermons/b.png"], "add_tags": "easter, Choir" })).await.unwrap();
    core.call(&token, "media.folder.add", json!({ "name": "Archive" })).await.unwrap();
    let cycle = core.call(&token, "media.folder.move", json!({ "name": "Sermons", "into": "Sermons/2026" })).await.unwrap_err();
    assert_eq!(cycle.data["field"], "into", "{cycle:?}");
    let moved = core.call(&token, "media.folder.move", json!({ "name": "Sermons", "into": "Archive", "force": true })).await.unwrap();
    assert!(moved["updated"].as_array().unwrap().iter().any(|u| u == "source pic2"), "{moved}");
    assert!(uri_of(&core, "pic2").await.starts_with("file:") && uri_of(&core, "pic2").await.contains("Archive/Sermons/b.png"));
    assert!(std::fs::read_to_string(dir.join("card.svg")).unwrap().contains("Archive/Sermons/b.png"), "the template follows");

    // The listing narrows by folder, subfolders, tag and words; tags moved.
    let all = core.call(&token, "media.list", json!({})).await.unwrap();
    assert!(all["folders"].as_array().unwrap().iter().any(|f| f == "Archive/Sermons/2026"), "{all}");
    let top = core.call(&token, "media.list", json!({ "folder": "Archive", "subfolders": true })).await.unwrap();
    assert_eq!(names(&top), ["Archive/Sermons/a.png", "Archive/Sermons/b.png"]);
    let tagged = core.call(&token, "media.list", json!({ "tag": "choir" })).await.unwrap();
    assert_eq!(names(&tagged), ["Archive/Sermons/b.png"]);
    let counts = core.call(&token, "media.folder.list", json!({})).await.unwrap();
    assert!(counts["tags"].as_array().unwrap().iter().any(|t| t["name"] == "Choir" && t["count"] == 1), "{counts}");

    // Removing a folder asks where its contents go, and deletes no file.
    let ask = core.call(&token, "media.folder.remove", json!({ "name": "Archive" })).await.unwrap_err();
    assert_eq!(ask.data["valid"], json!(["up", "top"]));
    core.call(&token, "media.folder.remove", json!({ "name": "Archive", "contents": "up", "force": true })).await.unwrap();
    assert!(dir.join("Sermons/b.png").is_file() && !dir.join("Archive").exists());
    assert!(uri_of(&core, "pic2").await.contains("/Sermons/b.png"));
    let bad = core.call(&token, "media.folder.add", json!({ "name": "../out" })).await.unwrap_err();
    assert_eq!(bad.data["field"], "name");
    drop(core);
    let _ = std::fs::remove_dir_all(&dir);
}
