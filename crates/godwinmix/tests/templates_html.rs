//! HTML templates through the method table, on a running core: listed with
//! the SVG ones, read, checked with a fix for each mistake, refused on save
//! when broken, saved when right. Nothing here draws one, so it needs no
//! browser renderer.

// The harness reads these through `super::*`; this file uses few of them.
#![allow(unused_imports)]

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_html_template_is_listed_checked_and_saved() {
    let dir = std::env::temp_dir().join(format!("gmx-templates-html-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let media = dir.display().to_string();
    let safety = godwinmix_core::safety::SafetyConfig::default();
    let core = Core::start_configured(safety, false, |cfg| cfg.media.dir = media.clone()).await;
    godwinmix_core::graphics::configure(&media, &Default::default());
    let token = desk();

    let list = core.call(&token, "template.list", json!({})).await.unwrap();
    let glass = list["templates"].as_array().unwrap().iter().find(|t| t["name"] == "lower-third-glass").expect("the HTML pack is listed").clone();
    assert_eq!((glass["format"].as_str(), glass["uri"].as_str(), glass["out_ms"].as_u64()), (Some("html"), Some("html:lower-third-glass"), Some(600)));

    let doc = core.call(&token, "template.get", json!({ "name": "lower-third-glass" })).await.unwrap();
    let page = doc["html"].as_str().expect("the page as written").to_string();
    let ok = core.call(&token, "template.check", json!({ "html": page })).await.unwrap();
    assert_eq!(ok["ok"], true, "{ok}");

    let painted = page.replace("background: transparent", "background: #000000");
    let bad = core.call(&token, "template.check", json!({ "html": painted })).await.unwrap();
    assert_eq!(bad["ok"], false);
    let fix = bad["problems"][0]["fix"].as_str().unwrap();
    assert!(fix.contains("background: transparent"), "the fix says what to write: {bad}");
    let refused = core.call(&token, "template.save", json!({ "name": "black", "html": painted })).await.unwrap_err();
    assert!(refused.message.contains("Fix:") && !dir.join("black.html").exists(), "{refused:?}");

    let ours = page.replace("Glass lower third", "Ours");
    let saved = core.call(&token, "template.save", json!({ "name": "ours", "html": ours })).await.unwrap();
    assert_eq!(saved["template"]["uri"], "html:ours.html", "{saved}");
    let listed = core.call(&token, "template.list", json!({})).await.unwrap();
    assert!(listed["templates"].as_array().unwrap().iter().any(|t| t["name"] == "ours.html" && t["origin"] == "library"));
    drop(core);
    let _ = std::fs::remove_dir_all(&dir);
}
