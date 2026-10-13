//! A clip moves in its tile and in the Studio preview at the rate asked for.
//!
//! A clip is not live: it decodes as fast as the programme's queues take it,
//! so its frames leave its tee most of a second before they are due. Each one
//! then waits at its mosaic pad, and the tile queue in front of that pad held
//! two frames and dropped the rest, so the tile showed one frame each time a
//! held one came due and froze in between. On 2026-10-13, a 1080p30 clip on a
//! 25 fps mosaic, release build on Windows: the clip's tile changed about 2
//! times a second while the programme tile beside it changed 25.
//!
//! Each frame carries its own timestamp in its picture (`barcode`), so the
//! number of different stamps the tile and the preview show over two seconds
//! is the number of different frames they drew.

use super::*;

const CLIP: &str = "pace-clip";
/// The mosaic and preview rate the page asks for on a machine with eight
/// threads or more.
const FPS: i32 = 25;
/// Different frames a second the tile and the preview must show. A 30 fps
/// clip on a 25 fps mosaic has 25 to give; a few may go to a busy runner.
const AT_LEAST: f64 = 18.0;

#[tokio::test(flavor = "multi_thread")]
async fn a_clip_moves_at_the_mosaic_rate_in_its_tile_and_the_preview() {
    let _ = gst::init();
    let Some(path) = write_clip() else {
        eprintln!("skipping: could not write a test clip with jpegenc and avimux");
        return;
    };
    let mut cfg: crate::config::Config = toml::from_str("").unwrap();
    cfg.canvas = crate::config::Canvas { width: 640, height: 360, fps: 30, sample_rate: 48000, channels: 2 };
    cfg.multiview = MultiviewConfig { width: 640, height: 360, fps: FPS, linger_secs: 0, ..Default::default() };
    let mut clip = crate::config::SourceConfig::bare(CLIP, &crate::input::file_uri(&path));
    clip.params.insert("at_end".into(), toml::Value::String("repeat".into()));
    cfg.sources = vec![clip];
    let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let (mv, preview, canvas) = (mix.multiview_handle(), mix.preview_handle(), mix.canvas().clone());
    let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
    preview.set_scene(vec![preview::Cell {
        source: CLIP.into(),
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
    let source = super::wait_for_pipeline(CLIP, Duration::from_secs(10).mul_f64(slack)).await;
    barcode::stamp_pts_at(&source, &format!("{CLIP}-vtee"));
    let _ = handle.send(Command::Take { source: Some(CLIP.into()), at_running_time_ms: None, ack: None });

    let mut mosaic = mv.subscribe(MultiviewRequest { fps: FPS, width: 640 });
    let mut pv = mv.subscribe_preview(PreviewRequest { fps: FPS, width: 640, full: false });
    tokio::time::sleep(Duration::from_secs(3).mul_f64(slack)).await;
    let cells = handle.status().await.unwrap().multiview.cells;
    let tile = cells.iter().find(|c| c.source.as_deref() == Some(CLIP)).expect("a tile").clone();

    let window = Duration::from_secs(2);
    let mut got: Vec<(Arc<[u8]>, bool)> = Vec::new();
    let end = Instant::now() + window;
    while Instant::now() < end {
        let (frame, is_preview) = tokio::select! {
            r = mosaic.recv() => (r, false),
            r = pv.recv() => (r, true),
            _ = tokio::time::sleep(Duration::from_millis(500)) => continue,
        };
        if let Ok(frame) = frame {
            got.push((frame, is_preview));
        }
    }
    drop((mosaic, pv));
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    let _ = std::fs::remove_dir_all(path.parent().unwrap());

    let (mut tiles, mut previews) = (Vec::new(), Vec::new());
    for (frame, is_preview) in got {
        let img = crate::snapshot::decode_jpeg(&frame).unwrap();
        if is_preview {
            previews.extend(barcode::read(&img));
        } else {
            tiles.extend(barcode::read(&barcode::cell_of(&img, &tile)));
        }
    }
    let secs = window.as_secs_f64();
    for (name, mut seen) in [("the clip's tile", tiles), ("the Studio preview", previews)] {
        seen.sort();
        seen.dedup();
        let rate = seen.len() as f64 / secs;
        eprintln!("{name}: {rate:.1} different frames a second");
        assert!(
            rate >= AT_LEAST / slack,
            "{name} drew {} different frames in {secs} s ({rate:.1} a second) where {FPS} were asked for: {seen:?}",
            seen.len()
        );
    }
}

/// Six seconds of a moving ball at 30 fps, in AVI with JPEG, which every
/// GStreamer install can write and which takes the ordinary decode path.
fn write_clip() -> Option<std::path::PathBuf> {
    let dir = std::env::temp_dir().join(format!("gmx-clip-rate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("clip.avi");
    let desc = format!(
        "videotestsrc num-buffers=180 pattern=ball ! video/x-raw,width=640,height=360,framerate=30/1 ! jpegenc ! \
         avimux ! filesink location=\"{}\"",
        path.display().to_string().replace('\\', "/")
    );
    let pipeline = gst::parse::launch(&desc).ok()?;
    pipeline.set_state(gst::State::Playing).ok()?;
    let done = pipeline.bus()?.timed_pop_filtered(
        gst::ClockTime::from_seconds(20),
        &[gst::MessageType::Eos, gst::MessageType::Error],
    );
    let _ = pipeline.set_state(gst::State::Null);
    done.filter(|m| m.type_() == gst::MessageType::Eos).map(|_| path)
}
