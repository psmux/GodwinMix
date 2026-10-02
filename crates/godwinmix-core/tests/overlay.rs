//! Transparent sources on a running programme, read back off the programme
//! itself: the pixels a viewer would get, not a property somebody set.

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_text_box_sits_over_the_picture_under_it_and_nowhere_else() {
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("bg", "test://blue")).await;
    let mut text = SourceConfig::bare("strap", "text:");
    text.params = toml::from_str("text = \"\"\nbackground = \"#ff0000\"\nradius = 0\nwidth = 100\nheight = 40").unwrap();
    add(&handle, text).await;
    take(&handle, vec![full("bg"), at("strap", 40, 100, 100, 40)]).await;
    settle(1_500).await;
    let f = frames.latest().expect("programme frames");
    let blue = f.yuv(10, 10);
    let red = f.yuv(90, 120);
    let beside = f.yuv(200, 120);
    stop(handle, thread);
    assert!(near(blue, (32, 240, 118)), "the background is blue: {blue:?}");
    assert!(near(red, (63, 102, 240)), "inside the box is red: {red:?}");
    assert!(near(beside, blue), "outside the box is the background: {beside:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_ticker_moves_across_its_bar_and_stays_inside_it() {
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("bg", "test://blue")).await;
    let mut ticker = SourceConfig::bare("crawl", "ticker:");
    ticker.params = toml::from_str("text = \"WWWWWWWWWW\"\nbackground = \"#ff0000\"\nsize = 20\nspeed = 200").unwrap();
    add(&handle, ticker).await;
    take(&handle, vec![full("bg"), at("crawl", 0, 140, 320, 40)]).await;
    settle(1_200).await;
    let row = |f: &support::Frame| (0..320).map(|x| f.yuv(x, 160).0).collect::<Vec<u8>>();
    let a = row(&frames.latest().unwrap());
    settle(300).await;
    let f = frames.latest().unwrap();
    let b = row(&f);
    let above = f.yuv(160, 100);
    stop(handle, thread);
    assert!(near(above, (32, 240, 118)), "above the bar is the background: {above:?}");
    assert!(a.iter().any(|y| *y > 150), "white letters are on the bar");
    assert_ne!(a, b, "the letters moved between two frames 300 ms apart");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changing_the_words_applies_in_place_with_no_rebuild_and_no_gap() {
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("bg", "test://blue")).await;
    let mut text = SourceConfig::bare("strap", "text:");
    text.params = toml::from_str("text = \"\"\nbackground = \"#ff0000\"\nradius = 0\nwidth = 100\nheight = 40").unwrap();
    add(&handle, text).await;
    take(&handle, vec![full("bg"), at("strap", 40, 100, 100, 40)]).await;
    settle(1_000).await;
    let mut events = handle.subscribe();
    frames.intervals.lock().unwrap().clear();
    let params: godwinmix_core::config::Params =
        toml::from_str("text = \"\"\nbackground = \"#00ff00\"\nradius = 0\nwidth = 100\nheight = 40").unwrap();
    let applied = handle
        .configure_source("strap".into(), params)
        .await
        .expect("the mixer answers");
    assert!(matches!(applied, godwinmix_core::plugin::Configure::Applied), "a text takes new words in place");
    settle(800).await;
    let green = frames.latest().unwrap().yuv(90, 120);
    let mut rebuilt = false;
    while let Ok(e) = events.try_recv() {
        rebuilt |= matches!(&e.event, godwinmix_core::state::Event::SourceStateChanged { source, .. } if source == "strap");
    }
    let worst = frames.worst_interval();
    stop(handle, thread);
    assert!(near(green, (173, 42, 26)), "the box turned green on the programme: {green:?}");
    assert!(!rebuilt, "the source was not rebuilt: it never went back through connecting");
    assert!(worst < 34.0 * 2.0 * godwinmix_core::plugin::harness::timing_slack(), "no gap: worst interval {worst:.1} ms");
}
