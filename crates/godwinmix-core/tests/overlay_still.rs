//! Transparent pictures on the programme: a PNG with alpha and an SVG, each
//! over a known colour, read back off the programme frames.

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

const BLUE: (u8, u8, u8) = (32, 240, 118);
const RED: (u8, u8, u8) = (63, 102, 240);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_transparent_png_shows_the_picture_under_its_clear_half() {
    let dir = scratch("png");
    let png = dir.join("logo.png");
    half_red_png(&png, 64, 36);
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("bg", "test://blue")).await;
    add(&handle, SourceConfig::bare("logo", &png.to_string_lossy())).await;
    take(&handle, vec![full("bg"), at("logo", 0, 0, 128, 72)]).await;
    settle(1_500).await;
    let f = frames.latest().expect("programme frames");
    let (opaque, clear, outside) = (f.yuv(20, 30), f.yuv(100, 30), f.yuv(200, 30));
    stop(handle, thread);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(near(opaque, RED), "the opaque half is red on the programme: {opaque:?}");
    assert!(near(clear, BLUE), "the clear half shows the blue under it, not black: {clear:?}");
    assert!(near(outside, BLUE), "{outside:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_svg_is_drawn_at_the_size_it_is_placed_at() {
    if !gst::init().is_ok() || gst::ElementFactory::find("rsvgdec").is_none() {
        println!("skipping: this GStreamer has no rsvgdec");
        return;
    }
    let dir = scratch("svg");
    let svg = dir.join("mark.svg");
    std::fs::write(&svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect x="0" y="0" width="20" height="20" fill="#ff0000"/></svg>"##).unwrap();
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("bg", "test://blue")).await;
    add(&handle, SourceConfig::bare("mark", &svg.to_string_lossy())).await;
    take(&handle, vec![full("bg"), at("mark", 0, 0, 160, 80)]).await;
    settle(1_500).await;
    let f = frames.latest().expect("programme frames");
    let (opaque, clear) = (f.yuv(40, 40), f.yuv(120, 40));
    stop(handle, thread);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(near(opaque, RED), "the drawn half is red: {opaque:?}");
    assert!(near(clear, BLUE), "the rest of the SVG is clear: {clear:?}");
}
