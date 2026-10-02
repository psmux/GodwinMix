//! A graphic template on a running programme: drawn over the picture, and a
//! field changed on air with no rebuild and no gap.

use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::slots::{Placement, Sizing};
use godwinmix_core::mixer::{self, Command, Mixer, MixerHandle, ProgramScene};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "overlay/support.rs"]
mod support;
use support::*;

/// A 320x180 template: a box filled with the `panel` field, words in
/// `headline` on it.
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 180">
  <rect x="40" y="100" width="200" height="40" fill="{{panel}}"/>
  <text x="50" y="128" font-size="20" fill="#ffffff" data-fit-width="180">{{headline}}</text>
</svg>"##;

fn template_source(dir: &std::path::Path, panel: &str) -> SourceConfig {
    let path = dir.join("strap.svg");
    std::fs::write(&path, SVG).unwrap();
    let mut cfg = SourceConfig::bare("strap", &format!("template:{}", path.display()));
    cfg.params = toml::from_str(&format!("[fields]\npanel = \"{panel}\"\nheadline = \"Polls close\"")).unwrap();
    cfg
}

fn have_rsvg() -> bool {
    gst::init().unwrap();
    let ok = gst::ElementFactory::find("rsvgdec").is_some();
    if !ok {
        println!("skipping: no rsvgdec");
    }
    ok
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_field_change_redraws_on_air_with_no_rebuild_and_no_gap() {
    if !have_rsvg() {
        return;
    }
    let dir = scratch("template");
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("bg", "test://blue")).await;
    add(&handle, template_source(&dir, "#ff0000")).await;
    take(&handle, vec![full("bg"), full("strap")]).await;
    settle(1_500).await;
    let red = frames.latest().expect("programme frames").yuv(200, 104);
    let outside = frames.latest().unwrap().yuv(10, 10);
    let mut events = handle.subscribe();
    frames.intervals.lock().unwrap().clear();
    let params = template_source(&dir, "#00ff00").params;
    let applied = handle.configure_source("strap".into(), params).await.expect("the mixer answers");
    assert!(matches!(applied, godwinmix_core::plugin::Configure::Applied), "a template takes new fields in place");
    settle(800).await;
    let green = frames.latest().unwrap().yuv(200, 104);
    let mut rebuilt = false;
    while let Ok(e) = events.try_recv() {
        rebuilt |= matches!(&e.event, godwinmix_core::state::Event::SourceStateChanged { source, .. } if source == "strap");
    }
    let worst = frames.worst_interval();
    stop(handle, thread);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(near(outside, (32, 240, 118)), "outside the graphic is the background: {outside:?}");
    assert!(near(red, (63, 102, 240)), "the panel is red on the programme: {red:?}");
    assert!(near(green, (173, 42, 26)), "the panel turned green on the programme: {green:?}");
    assert!(!rebuilt, "the source was not rebuilt: it never went back through connecting");
    assert!(worst < 34.0 * 2.0 * godwinmix_core::plugin::harness::timing_slack(), "no gap: worst interval {worst:.1} ms");
}
