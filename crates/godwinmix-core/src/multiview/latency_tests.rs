//! How late the operator sees a frame: the budget the preview path is held to.
//!
//! Every frame leaving the source's normaliser has the programme's running
//! time written into its picture (`barcode`). The test reads it back out of
//! the source's mosaic tile, the programme tile and the Studio preview, the
//! three pictures the page shows, and compares it with the running time at
//! which the JPEG reached a subscriber. That difference is what the mosaic
//! and the preview add, with real elements and no mocks. Frames are stamped
//! as they arrive and decoded afterwards, so a slow decode in a debug build is
//! never counted as the mixer's.
//!
//! Measured on 2026-10-06 on a debug build (Windows, 16 threads): MEASURED.

use super::*;
use crate::mixer::Command;
use std::time::Duration;

mod barcode;

const ID: &str = "lat-cam";
/// What each picture may add, median over two seconds of frames.
const TILE_BUDGET_MS: i64 = 150;
const PROGRAMME_BUDGET_MS: i64 = 200;
const PREVIEW_BUDGET_MS: i64 = 150;

#[tokio::test(flavor = "multi_thread")]
async fn the_preview_path_adds_no_more_than_its_budget() {
    let _ = gst::init();
    let mut cfg: crate::config::Config = toml::from_str("").unwrap();
    cfg.canvas = crate::config::Canvas { width: 640, height: 360, fps: 30, sample_rate: 48000, channels: 2 };
    cfg.multiview = MultiviewConfig { width: 640, height: 360, fps: 8, linger_secs: 0, ..Default::default() };
    cfg.sources = vec![crate::config::SourceConfig::bare(ID, "test://smpte")];
    let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let (mv, preview, canvas) = (mix.multiview_handle(), mix.preview_handle(), mix.canvas().clone());
    let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
    preview.set_scene(vec![preview::Cell {
        source: ID.into(),
        x: 0,
        y: 0,
        width: canvas.width,
        height: canvas.height,
        alpha: 1.0,
        rotation: 0.0,
        crop: (0.0, 0.0, 0.0, 0.0),
        sizing: Default::default(),
    }]);
    let slack = crate::plugin::harness::timing_slack();
    let source = wait_for_source(Duration::from_secs(10).mul_f64(slack)).await;
    barcode::stamp_frames_at(&source, &format!("{ID}-vtee"));
    let _ = handle.send(Command::Take { source: Some(ID.into()), at_running_time_ms: None, ack: None });

    let mut mosaic = mv.subscribe(MultiviewRequest::configured());
    let mut pv = mv.subscribe_preview(PreviewRequest { fps: 8, width: 640, full: false });
    tokio::time::sleep(Duration::from_secs(3).mul_f64(slack)).await;
    let cells = handle.status().await.unwrap().multiview.cells;
    let tile = cells.iter().find(|c| c.source.as_deref() == Some(ID)).expect("a tile").clone();
    let programme = cells.iter().find(|c| c.source.is_none()).expect("the programme tile").clone();

    let mut got: Vec<(i64, Arc<[u8]>, bool)> = Vec::new();
    let end = Instant::now() + Duration::from_secs(2);
    while Instant::now() < end {
        // A reader that fell behind is told so once and then reads on. A
        // pattern that matched only `Ok` would wait out the timer instead.
        let (frame, is_preview) = tokio::select! {
            r = mosaic.recv() => (r, false),
            r = pv.recv() => (r, true),
            _ = tokio::time::sleep(Duration::from_millis(500)) => continue,
        };
        if let (Ok(frame), Some(now)) = (frame, barcode::running_now(&source)) {
            got.push((now, frame, is_preview));
        }
    }
    drop((mosaic, pv));
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();

    let (mut tiles, mut programmes, mut previews) = (Vec::new(), Vec::new(), Vec::new());
    for (now, frame, is_preview) in got {
        let img = crate::snapshot::decode_jpeg(&frame).unwrap();
        if is_preview {
            previews.extend(barcode::read(&img).map(|t| now - t));
        } else {
            tiles.extend(barcode::read(&barcode::cell_of(&img, &tile)).map(|t| now - t));
            programmes.extend(barcode::read(&barcode::cell_of(&img, &programme)).map(|t| now - t));
        }
    }
    let budget = |ms: i64| (ms as f64 * slack) as i64;
    for (name, mut seen, limit) in [
        ("the source tile", tiles, TILE_BUDGET_MS),
        ("the programme tile", programmes, PROGRAMME_BUDGET_MS),
        ("the Studio preview", previews, PREVIEW_BUDGET_MS),
    ] {
        seen.sort();
        assert!(seen.len() >= 4, "{name} showed a readable stamp {} times in two seconds", seen.len());
        let median = seen[seen.len() / 2];
        eprintln!("{name}: median {median} ms behind over {} frames", seen.len());
        assert!(median <= budget(limit), "{name} was {median} ms behind; the budget is {limit} ms: {seen:?}");
    }
}

async fn wait_for_source(within: Duration) -> gst::Pipeline {
    let name = format!("input-{ID}");
    let end = Instant::now() + within;
    loop {
        let found = crate::observe::introspect::pipeline(&name)
            .filter(|p| p.by_name(&format!("{ID}-vtee")).is_some());
        if let Some(p) = found {
            return p;
        }
        assert!(Instant::now() < end, "no pipeline called {name} within {within:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
