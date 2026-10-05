//! HTML templates on the programme, through the real browser renderer: the
//! page's clear parts show the picture under it, the programme taking it
//! plays its way in, and a moving page moves on air.
//!
//! Needs the renderer. `GMX_TEST_BROWSER` names it; otherwise the mixer's own
//! lookup is used, and with none found each test says so and passes.

use godwinmix_core::config::{BrowserConfig, Config, SourceConfig};
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

/// The renderer, as a `[browser]` section, or None to skip.
fn renderer() -> Option<String> {
    let path = std::env::var("GMX_TEST_BROWSER")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| godwinmix_core::setup::web::lookup(&BrowserConfig::default()).found)?;
    path.is_file().then(|| format!("\n[browser]\nsidecar = '{}'\n", path.display()))
}

/// A template page: `body` inside a 1920 by 1080 layout, with `css`.
fn page(dir: &std::path::Path, name: &str, css: &str, body: &str) -> String {
    let html = format!(
        r##"<!doctype html><html><head>
<script type="application/json" id="gmx-template">{{"title": "test", "out_ms": 0, "fields": {{"accent": {{"type": "color", "default": "#ff0000"}}}}}}</script>
<style>html, body {{ margin: 0; background: transparent; overflow: hidden; }} .box {{ position: absolute; background: var(--accent); }} {css}</style>
</head><body>{body}</body></html>"##
    );
    let path = dir.join(format!("{name}.html"));
    std::fs::write(&path, html).unwrap();
    format!("html:{}", path.display())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_page_is_clear_where_it_paints_nothing_and_comes_in_when_taken() {
    let Some(browser) = renderer() else {
        println!("skipping: no browser renderer here; set GMX_TEST_BROWSER");
        return;
    };
    let dir = scratch("html-alpha");
    // The left half of the page red, shown only once the graphic is in.
    let uri = page(&dir, "half", ".box { left: 0; top: 0; width: 960px; height: 1080px; opacity: 0; } .gmx-in .box { opacity: 1; }", "<div class=box></div>");
    let (handle, frames, thread, _) = running_with(&browser);
    add(&handle, SourceConfig::bare("bg", "test://blue")).await;
    add(&handle, SourceConfig::bare("gfx", &uri)).await;
    take(&handle, vec![full("bg")]).await;
    settle(6_000).await;
    take(&handle, vec![full("bg"), full("gfx")]).await;
    let shown = wait_for(&frames, |f| near(f.yuv(60, 90), RED), Duration::from_secs(10)).await;
    let f = frames.latest().expect("programme frames");
    let (painted, clear) = (f.yuv(60, 90), f.yuv(260, 90));
    stop(handle, thread);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(shown, "taking the scene played the page's way in: {painted:?}");
    assert!(near(painted, RED), "the painted half is red on the programme: {painted:?}");
    assert!(near(clear, BLUE), "the clear half shows the blue under it: {clear:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_moving_page_moves_on_air() {
    let Some(browser) = renderer() else {
        println!("skipping: no browser renderer here; set GMX_TEST_BROWSER");
        return;
    };
    let dir = scratch("html-motion");
    // A red box crossing the page over six seconds once it is in.
    let css = ".box { left: 0; top: 360px; width: 240px; height: 360px; } \
               .gmx-in .box { animation: go 6s linear forwards; } \
               @keyframes go { to { transform: translateX(1680px); } }";
    let uri = page(&dir, "moving", css, "<div class=box></div>");
    let (handle, frames, thread, _) = running_with(&browser);
    add(&handle, SourceConfig::bare("bg", "test://blue")).await;
    add(&handle, SourceConfig::bare("gfx", &uri)).await;
    take(&handle, vec![full("bg")]).await;
    settle(6_000).await;
    take(&handle, vec![full("bg"), full("gfx")]).await;
    wait_for(&frames, |f| red_at(f).is_some(), Duration::from_secs(10)).await;
    settle(1_000).await;
    let first = frames.latest().and_then(|f| red_at(&f));
    settle(2_000).await;
    let later = frames.latest().and_then(|f| red_at(&f));
    stop(handle, thread);
    let _ = std::fs::remove_dir_all(&dir);
    let (a, b) = (first.expect("the box is on air"), later.expect("the box is still on air"));
    // 280 page pixels a second is about 47 canvas pixels a second at 320 wide.
    assert!(b > a + 40, "the box moved right between two frames two seconds apart: {a} then {b}");
}

/// The left edge of the red box on the middle row, in canvas pixels.
fn red_at(f: &Frame) -> Option<usize> {
    (0..320).find(|x| near(f.yuv(*x, 90), RED))
}

async fn wait_for(frames: &Frames, ok: impl Fn(&Frame) -> bool, within: Duration) -> bool {
    let until = std::time::Instant::now() + within;
    while std::time::Instant::now() < until {
        if frames.latest().is_some_and(|f| ok(&f)) {
            return true;
        }
        settle(100).await;
    }
    false
}
