//! A keyed presenter between a background and a desk, on a running
//! programme, read back off the programme frames.
//!
//! The presenter is `test://smpte` keyed on its green bar: the green bar
//! must show the background through it, the white bar must stay white, and a
//! transparent PNG above the presenter in the stack must cover it.

use godwinmix_core::config::{Config, Params, SourceConfig};
use godwinmix_core::mixer::slots::{ItemFilter, Placement, Sizing};
use godwinmix_core::mixer::{self, Command, Mixer, MixerHandle, ProgramScene};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "overlay/support.rs"]
mod support;
use support::*;

const BLUE: (u8, u8, u8) = (32, 240, 118);
const RED: (u8, u8, u8) = (63, 102, 240);

/// Where the bars of `test://smpte` are on the 320 wide canvas, in the top
/// two thirds: each is a seventh of the width.
const WHITE_BAR: usize = 20;
const GREEN_BAR: usize = 160;
const RED_BAR: usize = 250;

fn keyed(id: &str, params: &str) -> Placement {
    let params: Params = toml::from_str(params).unwrap();
    Placement {
        filters: vec![ItemFilter { type_id: "chroma/filter".into(), name: None, params }],
        ..full(id)
    }
}

fn is_white(p: (u8, u8, u8)) -> bool {
    p.0 > 160 && (p.1 as i32 - 128).abs() < 10 && (p.2 as i32 - 128).abs() < 10
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_key_shows_the_background_through_the_screen_and_the_desk_stays_in_front() {
    let dir = scratch("virtual-set");
    let desk = dir.join("desk.png");
    half_red_png(&desk, 64, 36);
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("set", "test://blue")).await;
    add(&handle, SourceConfig::bare("presenter", "test://smpte")).await;
    add(&handle, SourceConfig::bare("desk", &desk.to_string_lossy())).await;
    let presenter = keyed("presenter", "color = \"#00c000\"");
    take(&handle, vec![full("set"), presenter, at("desk", 0, 100, 100, 40)]).await;
    settle(1_500).await;
    let f = frames.latest().expect("programme frames");
    let (screen, white, in_front) = (f.yuv(GREEN_BAR, 40), f.yuv(WHITE_BAR, 60), f.yuv(WHITE_BAR, 110));
    stop(handle, thread);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(near(screen, BLUE), "the green bar shows the set behind it, not black: {screen:?}");
    assert!(is_white(white), "the white bar is still the presenter: {white:?}");
    assert!(near(in_front, RED), "the desk is drawn in front of the presenter: {in_front:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_change_to_the_key_applies_on_air_with_no_rebuild_and_no_gap() {
    let (handle, frames, thread, pipeline) = running_with_pipeline();
    add(&handle, SourceConfig::bare("set", "test://blue")).await;
    add(&handle, SourceConfig::bare("presenter", "test://smpte")).await;
    take(&handle, vec![full("set"), keyed("presenter", "color = \"#00c000\"")]).await;
    settle(1_200).await;
    let before = frames.latest().unwrap().yuv(RED_BAR, 60);
    let bin = key_bins(&pipeline);
    frames.intervals.lock().unwrap().clear();
    // A second on air with the key in place first: a pad fed nothing at all
    // would stall the programme here, which is why the key sends gaps.
    settle(1_000).await;
    // The garbage matte pulled in from the right: the red bar is outside it.
    take(&handle, vec![full("set"), keyed("presenter", "color = \"#00c000\"\nmatte_right = 0.4")]).await;
    settle(800).await;
    let f = frames.latest().unwrap();
    let (after, white) = (f.yuv(RED_BAR, 60), f.yuv(WHITE_BAR, 60));
    let worst = frames.worst_interval();
    let bin_after = key_bins(&pipeline);
    stop(handle, thread);
    assert!(!near(before, BLUE), "before, the red bar is the presenter: {before:?}");
    assert!(near(after, BLUE), "after, the matte keys it away: {after:?}");
    assert!(is_white(white), "inside the matte nothing changed: {white:?}");
    assert_eq!(bin.len(), 1, "one key in the programme: {bin:?}");
    assert_eq!(bin, bin_after, "the same key bin, changed in place rather than built again");
    let slack = godwinmix_core::plugin::harness::timing_slack();
    assert!(worst < 34.0 * 2.0 * slack, "no gap on the programme: worst interval {worst:.1} ms");
}

/// The names of the key bins in the programme. Each build takes a new name,
/// so the same name before and after means the same bin.
fn key_bins(pipeline: &gst::Pipeline) -> Vec<String> {
    pipeline
        .iterate_recurse()
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.name().to_string())
        .filter(|n| n.starts_with("filter-chroma-") && !n.ends_with("-key"))
        .collect()
}
