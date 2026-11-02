//! An item's filters reach the preview before Take, read back off the
//! preview's own JPEGs. Real sources, the real mosaic pipeline and the real
//! filters; nothing is taken to air, so whatever changes is the preview's.
//!
//! The rule under test is the owner's: every change is seen in Preview before
//! it goes live. It used to fail for every filter, because the preview drew
//! an item from its source's thumbnail with the geometry and nothing else.

use super::*;
use crate::config::Params;
use crate::mixer::slots::ItemFilter;
use crate::mixer::Command;
use std::time::Duration;

fn cfg(sources: &[(&str, &str)]) -> crate::config::Config {
    let mut cfg: crate::config::Config = toml::from_str("").unwrap();
    cfg.canvas = crate::config::Canvas {
        width: 320,
        height: 180,
        fps: crate::config::FrameRate::whole(15),
        sample_rate: 48000,
        channels: 2,
    };
    cfg.multiview = MultiviewConfig { width: 320, height: 180, fps: 8, linger_secs: 0, ..Default::default() };
    cfg.sources = sources.iter().map(|(id, uri)| crate::config::SourceConfig::bare(id, uri)).collect();
    cfg
}

fn filter(type_id: &str, params: &str) -> ItemFilter {
    let params: Params = toml::from_str(params).unwrap();
    ItemFilter { type_id: type_id.into(), name: None, params }
}

fn full(source: &str, filters: Vec<ItemFilter>) -> preview::Cell {
    preview::Cell {
        source: source.into(),
        x: 0,
        y: 0,
        width: 320,
        height: 180,
        alpha: 1.0,
        rotation: 0.0,
        crop: (0.0, 0.0, 0.0, 0.0),
        sizing: crate::mixer::slots::Sizing::Fill,
        filters,
    }
}

/// How much of the picture has colour in it, 0 to 1.
fn colourful(img: &image::RgbImage) -> f64 {
    let n = img.pixels().filter(|p| p.0.iter().max().unwrap() - p.0.iter().min().unwrap() > 60).count();
    n as f64 / (img.width() * img.height()) as f64
}

/// The colour at a fraction of the way across and down.
fn at(img: &image::RgbImage, fx: f64, fy: f64) -> (u8, u8, u8) {
    let p = img.get_pixel((fx * img.width() as f64) as u32, (fy * img.height() as f64) as u32).0;
    (p[0], p[1], p[2])
}

/// Preview frames until one passes `want`, or the last one seen when none
/// does inside `within`.
async fn until(sub: &mut PreviewSubscription, within: Duration, want: impl Fn(&image::RgbImage) -> bool) -> (bool, Option<image::RgbImage>) {
    let end = Instant::now() + within.mul_f64(crate::plugin::harness::timing_slack());
    let mut last = None;
    while Instant::now() < end {
        let left = end.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, sub.recv()).await {
            Ok(Ok(jpeg)) => {
                let img = crate::snapshot::decode_jpeg(&jpeg).unwrap();
                if want(&img) {
                    return (true, Some(img));
                }
                last = Some(img);
            }
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
            _ => break,
        }
    }
    (false, last)
}

/// Filter bins in the mosaic pipeline, which is where the preview lives.
fn preview_filter_bins(mv: &MultiviewHandle) -> usize {
    let Some(p) = mv.pipeline() else { return 0 };
    p.iterate_elements().into_iter().filter_map(Result::ok).filter(|e| e.name().starts_with("filter-")).count()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_colour_filter_on_an_item_changes_the_preview_before_take_and_follows_its_settings() {
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg(&[("cam1", "test://smpte")])).unwrap();
    mix.start().unwrap();
    let (mv, preview) = (mix.multiview_handle(), mix.preview_handle());
    let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
    preview.set_scene(vec![full("cam1", Vec::new())]);
    let mut sub = mv.subscribe_preview(PreviewRequest { fps: 8, width: 320, full: false });

    let (bars, _) = until(&mut sub, Duration::from_secs(10), |img| colourful(img) > 0.4).await;
    // Saturation -1 is OBS's grey: the bars lose their colour and keep their
    // brightness.
    preview.set_scene(vec![full("cam1", vec![filter("color/filter", "saturation = -1.0")])]);
    let (grey, grey_img) = until(&mut sub, Duration::from_secs(5), |img| colourful(img) < 0.02).await;
    let bins_grey = preview_filter_bins(&mv);
    // The same filter with another setting is changed in place, which is a
    // slider dragged in the composer: no new bin.
    preview.set_scene(vec![full("cam1", vec![filter("color/filter", "saturation = 0.0")])]);
    let (back, _) = until(&mut sub, Duration::from_secs(5), |img| colourful(img) > 0.4).await;
    let bins_back = preview_filter_bins(&mv);
    // And the filter taken off takes its bin with it.
    preview.set_scene(vec![full("cam1", Vec::new())]);
    let (_, _) = until(&mut sub, Duration::from_secs(2), |img| colourful(img) > 0.4).await;
    let bins_none = preview_filter_bins(&mv);

    drop(sub);
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert!(bars, "the preview never showed the colour bars");
    assert!(grey, "the colour filter did not reach the preview: {:?} of it still colourful", grey_img.map(|i| colourful(&i)));
    assert_eq!(bins_grey, 1, "one filter bin for one filter on one item");
    assert!(back, "a change of setting did not reach the preview");
    assert_eq!(bins_back, 1, "a change of setting built a second bin rather than changing the first");
    assert_eq!(bins_none, 0, "a filter taken off the item left its bin in the preview");
}

/// The rig's report, at the size of a test: a presenter keyed in front of a
/// set showed her screen in Preview while Programme showed the set.
/// `test://smpte`'s green bar is the screen and `test://blue` is the set.
#[tokio::test(flavor = "multi_thread")]
async fn a_chroma_key_on_an_item_shows_the_set_through_the_screen_in_the_preview() {
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, _bus_rx) =
        crate::mixer::Mixer::build(cfg(&[("set", "test://blue"), ("presenter", "test://smpte")])).unwrap();
    mix.start().unwrap();
    let (mv, preview) = (mix.multiview_handle(), mix.preview_handle());
    let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
    // The green bar is the fourth of seven, in the top two thirds.
    let (gx, gy, wx) = (3.5 / 7.0, 0.3, 0.5 / 7.0);
    let green = |p: (u8, u8, u8)| p.1 > 120 && p.0 < 90 && p.2 < 90;
    let blue = |p: (u8, u8, u8)| p.2 > 160 && p.0 < 90 && p.1 < 90;
    let white = |p: (u8, u8, u8)| p.0 > 150 && p.1 > 150 && p.2 > 150;

    preview.set_scene(vec![full("set", Vec::new()), full("presenter", Vec::new())]);
    let mut sub = mv.subscribe_preview(PreviewRequest { fps: 8, width: 320, full: false });
    let (screen, before) = until(&mut sub, Duration::from_secs(10), |img| green(at(img, gx, gy))).await;
    preview.set_scene(vec![
        full("set", Vec::new()),
        full("presenter", vec![filter("chroma/filter", "color = \"#00c000\"")]),
    ]);
    let (keyed, after) = until(&mut sub, Duration::from_secs(5), |img| blue(at(img, gx, gy))).await;
    let presenter_kept = after.as_ref().map(|img| at(img, wx, gy));

    drop(sub);
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert!(screen, "the unkeyed presenter's green bar never showed: {:?}", before.map(|i| at(&i, gx, gy)));
    assert!(keyed, "the key did not reach the preview: the green bar reads {:?}", after.as_ref().map(|i| at(i, gx, gy)));
    let kept = presenter_kept.unwrap();
    assert!(white(kept), "the key took the white bar away as well: {kept:?}");
}

/// A look from the LUT library, the composer's "Look" filter, on the armed
/// item: Preview goes black and white before anything is taken.
#[tokio::test(flavor = "multi_thread")]
async fn a_look_on_an_item_reaches_the_preview_before_take() {
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg(&[("cam1", "test://smpte")])).unwrap();
    mix.start().unwrap();
    let (mv, preview) = (mix.multiview_handle(), mix.preview_handle());
    let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
    preview.set_scene(vec![full("cam1", vec![filter("lut/filter", "lut = \"black-and-white\"")])]);
    let mut sub = mv.subscribe_preview(PreviewRequest { fps: 8, width: 320, full: false });
    // Bright enough to be the bars, with no colour in them.
    let graded = |img: &image::RgbImage| colourful(img) < 0.02 && at(img, 0.5 / 7.0, 0.3).0 > 120;
    let (seen, last) = until(&mut sub, Duration::from_secs(10), graded).await;
    drop(sub);
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert!(seen, "the look did not reach the preview: {:?} colourful", last.map(|i| colourful(&i)));
}

/// The person cutout, with the real model: `test://smpte` has no person in
/// it, so once the model answers the whole presenter is cut away and the set
/// shows where the bars were. Then the same scene goes to air, and the
/// programme and the preview share one model on the presenter's picture
/// rather than running two. Skips on a machine with no runtime or models,
/// as `tests/matte.rs` does.
#[cfg(feature = "matte")]
#[tokio::test(flavor = "multi_thread")]
async fn a_cutout_reaches_the_preview_and_shares_its_model_with_the_programme() {
    use crate::plugin::filters::matte;
    let params = "quality = \"fast\"\ndevice = \"cpu\"";
    if let Err(e) = matte::ready(&toml::from_str(params).unwrap()) {
        eprintln!("skipped: {e:#}");
        return;
    }
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, _bus_rx) =
        crate::mixer::Mixer::build(cfg(&[("set", "test://blue"), ("cut-presenter", "test://smpte")])).unwrap();
    mix.start().unwrap();
    let (mv, preview, canvas) = (mix.multiview_handle(), mix.preview_handle(), mix.canvas().clone());
    let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
    let blue = |p: (u8, u8, u8)| p.2 > 160 && p.0 < 90 && p.1 < 90;
    let cut = filter("matte/filter", params);
    preview.set_scene(vec![full("set", Vec::new()), full("cut-presenter", vec![cut.clone()])]);
    let mut sub = mv.subscribe_preview(PreviewRequest { fps: 8, width: 320, full: false });
    let white_bar = (0.5 / 7.0, 0.3);
    let (gone, last) = until(&mut sub, Duration::from_secs(20), |img| blue(at(img, white_bar.0, white_bar.1))).await;
    let wide = crate::plugin::filter::share_key("cut-presenter", (0.0, 0.0, 0.0, 0.0), 0.0);
    let armed_only = matte::workers_on(&wide);

    // On air as well: the programme's copy leads and the preview's follows.
    let placement = |id: &str, filters: Vec<ItemFilter>| crate::mixer::slots::Placement {
        sizing: crate::mixer::slots::Sizing::Fill,
        filters,
        ..crate::mixer::slots::Placement::full_canvas(id.into(), &canvas)
    };
    let scene = crate::mixer::ProgramScene {
        name: "studio".into(),
        placements: vec![placement("set", Vec::new()), placement("cut-presenter", vec![cut])],
    };
    handle
        .request(|ack| Command::TakeScene { scene: Box::new(scene), at_running_time_ms: None, duration_ms: None, transition: None, ack: Some(ack) })
        .await
        .expect("take the scene");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let both = matte::workers_on(&wide);
    let (still_gone, _) = until(&mut sub, Duration::from_secs(5), |img| blue(at(img, white_bar.0, white_bar.1))).await;

    drop(sub);
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert!(gone, "the cutout did not reach the preview: the white bar reads {:?}", last.map(|i| at(&i, white_bar.0, white_bar.1)));
    assert_eq!(armed_only, 1, "one model for the armed presenter");
    assert_eq!(both, 1, "the presenter on air and in Preview ran {both} models, not one");
    assert!(still_gone, "the preview lost its cutout once the scene went to air");
}

/// The review's case: the same camera wide and cropped in one scene, each
/// with a cutout, on air and in Preview. Two pictures, so two models, one
/// each, and neither copy reads the other's masks. Keyed by the source alone
/// they shared one model and laid the wide shot's mask on the crop.
#[cfg(feature = "matte")]
#[tokio::test(flavor = "multi_thread")]
async fn the_same_camera_wide_and_cropped_runs_one_model_per_picture() {
    use crate::plugin::filter::share_key;
    use crate::plugin::filters::matte;
    let params = "quality = \"fast\"
device = \"cpu\"";
    if let Err(e) = matte::ready(&toml::from_str(params).unwrap()) {
        eprintln!("skipped: {e:#}");
        return;
    }
    let _ = gst::init();
    let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg(&[("crop-cam", "test://smpte")])).unwrap();
    mix.start().unwrap();
    let (mv, preview, canvas) = (mix.multiview_handle(), mix.preview_handle(), mix.canvas().clone());
    let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
    let cut = filter("matte/filter", params);
    let tight = (0.25, 0.0, 0.25, 0.0);
    let cropped = preview::Cell { crop: tight, ..full("crop-cam", vec![cut.clone()]) };
    preview.set_scene(vec![full("crop-cam", vec![cut.clone()]), cropped]);
    let mut sub = mv.subscribe_preview(PreviewRequest { fps: 8, width: 320, full: false });
    let _ = until(&mut sub, Duration::from_secs(5), |_| false).await;
    let placement = |crop, filters: Vec<ItemFilter>| crate::mixer::slots::Placement {
        sizing: crate::mixer::slots::Sizing::Fill,
        filters,
        crop,
        ..crate::mixer::slots::Placement::full_canvas("crop-cam".into(), &canvas)
    };
    let scene = crate::mixer::ProgramScene {
        name: "two-shots".into(),
        placements: vec![placement((0.0, 0.0, 0.0, 0.0), vec![cut.clone()]), placement(tight, vec![cut])],
    };
    handle
        .request(|ack| Command::TakeScene { scene: Box::new(scene), at_running_time_ms: None, duration_ms: None, transition: None, ack: Some(ack) })
        .await
        .expect("take the scene");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let wide = matte::workers_on(&share_key("crop-cam", (0.0, 0.0, 0.0, 0.0), 0.0));
    let narrow = matte::workers_on(&share_key("crop-cam", tight, 0.0));
    let bins = preview_filter_bins(&mv);

    drop(sub);
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert_eq!(bins, 2, "both preview copies have their cutout");
    assert_eq!(wide, 1, "the wide shot on air and in Preview ran {wide} models, not one");
    assert_eq!(narrow, 1, "the cropped shot on air and in Preview ran {narrow} models, not one");
}
