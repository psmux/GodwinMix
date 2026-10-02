//! Graphic templates through the method table, on a running core: what an
//! agent, the CLI and the web UI all do to put a designed graphic on air and
//! change it there.

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

const BAR: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:gmx="https://godwinmix.dev/ns/template" viewBox="0 0 320 180">
  <metadata><gmx:field name="headline" label="Headline" default="Hello"/></metadata>
  <rect x="10" y="130" width="300" height="40" fill="{{accent}}"/>
  <text x="20" y="158" font-size="20" fill="#ffffff" data-fit-width="280">{{headline}}</text>
</svg>"##;

/// A core whose media library is a folder of its own. The two tests share
/// the process, and with it the library the graphics read, so both use the
/// same folder and neither empties it.
async fn core_with_library() -> (Core, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("gmx-templates-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let media = dir.display().to_string();
    let safety = godwinmix_core::safety::SafetyConfig::default();
    let core = Core::start_configured(safety, false, |cfg| cfg.media.dir = media.clone()).await;
    godwinmix_core::graphics::configure(&media, &Default::default());
    (core, dir)
}

async fn live(core: &Core, token: &Token, id: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while core.call(token, "source.get", json!({ "id": id })).await.unwrap()["state"] != "live" {
        assert!(std::time::Instant::now() < deadline, "{id} never went live");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn field<'a>(fields: &'a Value, name: &str) -> &'a Value {
    fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == name).unwrap_or_else(|| panic!("no field {name}: {fields}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pack_graphic_goes_on_and_one_field_changes_in_place() {
    if gstreamer::init().is_err() || gstreamer::ElementFactory::find("rsvgdec").is_none() {
        println!("skipping: no rsvgdec");
        return;
    }
    let (core, _dir) = core_with_library().await;
    let token = desk();
    let list = core.call(&token, "template.list", json!({})).await.unwrap();
    let names: Vec<&str> = list["templates"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"news-lower-third") && names.contains(&"score-bug"), "{names:?}");

    let add = json!({ "id": "strap", "uri": "template:news-lower-third", "params": { "fields": { "name": "Ada" } } });
    core.call(&token, "source.add", add).await.unwrap();
    live(&core, &token, "strap").await;
    let set = core.call(&token, "source.set", json!({ "id": "strap", "params": { "fields": { "title": "Engineer" } } })).await.unwrap();
    assert_eq!(set["state"], "live", "a field change must not rebuild the graphic: {set}");
    let fields = core.call(&token, "template.fields", json!({ "id": "strap" })).await.unwrap();
    assert_eq!(field(&fields, "name")["value"], "Ada", "the field not named stays: {fields}");
    assert_eq!(field(&fields, "title")["value"], "Engineer");
    assert_eq!(field(&fields, "accent")["set"], false, "a colour left alone shows the default");
    assert_eq!(fields["path"], "params.fields.<name>");

    let bad = core.call(&token, "source.set", json!({ "id": "strap", "params": { "fields": { "headline": "x" } } })).await.unwrap_err();
    assert_eq!(bad.code, -32602, "{bad:?}");
    assert_eq!(bad.data["field"], "headline", "{bad:?}");
    assert!(bad.data["fields"].as_array().unwrap().iter().any(|f| f == "name"), "the error names the fields there are: {bad:?}");

    core.call(&token, "source.set", json!({ "id": "strap", "params": { "fields": { "name": null } } })).await.unwrap();
    let fields = core.call(&token, "template.fields", json!({ "id": "strap" })).await.unwrap();
    assert_eq!(field(&fields, "name")["value"], "Ada Lovelace", "null puts a field back to its default: {fields}");
    assert_eq!(field(&fields, "title")["value"], "Engineer");
    drop(core);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_saved_template_is_listed_added_and_redrawn_when_replaced() {
    if gstreamer::init().is_err() || gstreamer::ElementFactory::find("rsvgdec").is_none() {
        println!("skipping: no rsvgdec");
        return;
    }
    let (core, dir) = core_with_library().await;
    let token = desk();
    let saved = core.call(&token, "template.save", json!({ "name": "my-bar", "svg": BAR })).await.unwrap();
    assert_eq!(saved["template"]["uri"], "template:my-bar.svg", "{saved}");
    assert!(dir.join("my-bar.svg").is_file());
    let names: Vec<Value> = saved["template"]["fields"].as_array().unwrap().iter().map(|f| f["name"].clone()).collect();
    assert_eq!(names, vec![json!("headline"), json!("accent")]);

    let again = core.call(&token, "template.save", json!({ "name": "my-bar", "svg": BAR })).await.unwrap_err();
    assert!(again.message.contains("replace"), "writing over a file is asked for: {again:?}");
    let remote = BAR.replace("</svg>", r#"<image href="https://example.com/x.png"/></svg>"#);
    let refused = core.call(&token, "template.save", json!({ "name": "net", "svg": remote })).await.unwrap_err();
    assert!(refused.message.contains("network") && !dir.join("net.svg").exists(), "{refused:?}");

    core.call(&token, "source.add", json!({ "id": "bar", "uri": "template:my-bar" })).await.unwrap();
    live(&core, &token, "bar").await;
    let listed = core.call(&token, "template.list", json!({})).await.unwrap();
    assert!(listed["templates"].as_array().unwrap().iter().any(|t| t["name"] == "my-bar.svg" && t["origin"] == "library"));
    let replaced = core.call(&token, "template.save", json!({ "name": "my-bar.svg", "svg": BAR.replace("Hello", "Bye"), "replace": true })).await.unwrap();
    assert_eq!(replaced["redrawn"], json!(["bar"]), "the source drawing it is drawn again: {replaced}");
    let fields = core.call(&token, "template.fields", json!({ "id": "bar" })).await.unwrap();
    assert_eq!(field(&fields, "headline")["value"], "Bye", "it reads the new file: {fields}");
    let doc = core.call(&token, "template.get", json!({ "name": "news-lower-third" })).await.unwrap();
    assert!(doc["svg"].as_str().unwrap().contains("{{name}}"));
    drop(core);
    let _ = std::fs::remove_file(dir.join("my-bar.svg"));
}
