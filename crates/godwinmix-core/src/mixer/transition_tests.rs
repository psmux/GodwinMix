//! The new transitions and item motions against a real programme.
//!
//! Real `test://` sources, the real compositor and the real slot pool, with a
//! probe on the compositor's own output reading pixels back. Solid red goes
//! out and solid green comes in, so a pixel says which scene it belongs to by
//! its V sample alone: red is about 240, green about 34, black and white sit
//! at 128 and are told apart by Y.

use super::slots::{PadState, Placement};
use super::tests::{scene, with_sources_cfg, Gaps};
use super::transition::{item, Easing, Kind, TransitionSpec};
use super::{Command, Mixer, ProgramScene};
use crate::caps::CanvasCaps;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

/// A mixer whose sources are named after their colour.
async fn coloured(names: &[&str]) -> Mixer {
    let cfg = super::tests::programme_config(crate::config::Accel::Software);
    let mut mix = with_sources_cfg(&[], cfg).await;
    for name in names {
        let cfg: crate::config::SourceConfig =
            toml::from_str(&format!("id = \"{name}\"\nuri = \"test://{name}\"\n")).expect("a source");
        mix.add_source(&cfg, None).expect("adding a coloured source");
    }
    for _ in 0..100 {
        let live = names.iter().all(|n| {
            mix.sources.iter().any(|s| s.input.id == *n && matches!(s.input.observed_state(), crate::state::SourceState::Live))
        });
        if live {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    mix
}

fn full(canvas: &CanvasCaps, source: &str) -> Placement {
    Placement::full_canvas(source.into(), canvas)
}

/// Every frame the compositor makes while it is watched: its running time and
/// what a handful of canvas points show, as (Y, V).
#[derive(Default)]
pub(super) struct Tap {
    frames: Mutex<Vec<(u64, Seen)>>,
}

/// What one frame showed at each watched point, as (Y, V).
type Seen = Vec<(u8, u8)>;

impl Tap {
    pub(super) fn watch(self: &Arc<Self>, vmix: &gst::Element, points: Vec<(i32, i32)>) {
        let me = self.clone();
        let pad = vmix.static_pad("src").expect("the compositor has a src pad");
        // The segment went past before this probe existed, so it starts from
        // the sticky copy the pad keeps.
        let sticky = pad
            .sticky_event::<gst::event::Segment>(0)
            .and_then(|e| e.segment().downcast_ref::<gst::ClockTime>().cloned());
        let segment: Mutex<Option<gst::FormattedSegment<gst::ClockTime>>> = Mutex::new(sticky);
        pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM, move |pad, info| {
            match &info.data {
                Some(gst::PadProbeData::Event(e)) => {
                    if let gst::EventView::Segment(sg) = e.view() {
                        *segment.lock() = sg.segment().downcast_ref::<gst::ClockTime>().cloned();
                    }
                }
                Some(gst::PadProbeData::Buffer(b)) => {
                    let rt = b.pts().and_then(|pts| segment.lock().as_ref().and_then(|s| s.to_running_time(pts)));
                    let info = pad.current_caps().and_then(|c| gstreamer_video::VideoInfo::from_caps(&c).ok());
                    if let (Some(rt), Some(info)) = (rt, info) {
                        if let Ok(frame) = gstreamer_video::VideoFrameRef::from_buffer_ref_readable(b.as_ref(), &info) {
                            let read = |plane: u32, x: i32, y: i32, sub: i32| {
                                let stride = frame.plane_stride()[plane as usize];
                                let data = frame.plane_data(plane).unwrap_or(&[]);
                                data.get(((y / sub) * stride + x / sub) as usize).copied().unwrap_or(0)
                            };
                            let seen = points.iter().map(|(x, y)| (read(0, *x, *y, 1), read(2, *x, *y, 2))).collect();
                            me.frames.lock().push((rt.nseconds(), seen));
                        }
                    }
                }
                _ => {}
            }
            gst::PadProbeReturn::Ok
        });
    }

    /// Milliseconds from `from` to the first frame where any point is
    /// green, the scene coming in, or `None` if none was.
    pub(super) fn first_green_after(&self, from: u64) -> Option<u64> {
        let frames = self.frames.lock();
        frames.iter().filter(|(t, _)| *t >= from).find(|(_, v)| v.iter().any(|p| is_green(*p))).map(|(t, _)| (t - from) / 1_000_000)
    }

    /// What the frame nearest to `at` showed.
    pub(super) fn at(&self, at: u64) -> Vec<(u8, u8)> {
        let frames = self.frames.lock();
        frames.iter().min_by_key(|(t, _)| t.abs_diff(at)).map(|(_, v)| v.clone()).expect("a frame was seen")
    }
}

fn is_red(p: (u8, u8)) -> bool {
    p.1 > 180
}

fn is_green(p: (u8, u8)) -> bool {
    p.1 < 80
}

/// Take `b` over `a` with a transition, watch the programme through it, and
/// land it. Answers the tap and the window.
async fn cross(mix: &mut Mixer, spec: TransitionSpec, points: Vec<(i32, i32)>) -> (Arc<Tap>, (u64, u64), u64) {
    let canvas = mix.canvas.clone();
    mix.take_scene(scene("a", vec![full(&canvas, "red")]), None).expect("the first scene");
    let gaps = Arc::new(Gaps::default());
    gaps.watch(&mix.venc_tee.static_pad("sink").expect("the encoder tee has a sink pad"));
    gaps.wait_for(10).await;
    gaps.largest.store(0, std::sync::atomic::Ordering::Relaxed);
    let tap = Arc::new(Tap::default());
    tap.watch(mix.pool.compositor(), points);
    let ms = spec.duration_ms;
    mix.take_scene_over(scene("b", vec![full(&canvas, "green")]), None, None, Some(spec)).expect("the take");
    let window = mix.transition_window().expect("a transition is on the canvas");
    tokio::time::sleep(Duration::from_millis(ms + 400)).await;
    mix.handle(Command::TransitionEnd { transition: mix.transition_id() }).expect("the end");
    gaps.wait_for(6).await;
    (tap, window, gaps.largest.load(std::sync::atomic::Ordering::Relaxed))
}

/// The end state is the taken scene and nothing else: one pad, where the
/// scene put it, and its crop handed back.
fn landed_on_green(mix: &Mixer) {
    assert_eq!(mix.pool.visible(), 1, "only the taken scene is drawn once it has landed");
    let slot = mix.pool.slots().iter().find(|s| s.showing()).expect("a pad is showing");
    assert_eq!(slot.source().map(String::as_str), Some("green"));
    let c = &mix.canvas;
    assert_eq!(slot.state(), PadState { xpos: 0, ypos: 0, width: c.width, height: c.height, alpha: 1.0 });
    assert!(mix.pool.driven_by_a_transition("xpos").is_empty() && mix.pool.driven_by_a_transition("alpha").is_empty());
}

/// Three hundred milliseconds, longer by `GODWINMIX_TIMING_SLACK` on a runner
/// that declares itself slow: the middle frame is read by its time, and on a
/// Windows runner a 300 ms zoom still showed the old scene at its centre
/// half way through.
///
/// Longer does not cure what fails here under load. With the whole mixer
/// suite running beside it, three times in thirteen runs on a Windows
/// laptop, the scene coming in was not drawn at all for the whole window
/// (3 s at a slack of 3) and appeared 400 ms after it ended, when the
/// transition settled. The message prints the window a tenth at a time and
/// when the new scene first showed. See STATUS.md, 2026-10-06.
fn spec(kind: Kind) -> TransitionSpec {
    let ms = (300.0 * crate::plugin::harness::timing_slack()) as u64;
    TransitionSpec::new(kind, ms)
}

/// Every new transition: the programme never misses a frame, the middle
/// frame is the mix the transition promises, and the end is the scene taken.
#[tokio::test(flavor = "multi_thread")]
async fn every_new_transition_keeps_the_frame_rate_and_lands_on_the_taken_scene() {
    use super::transition::{Direction, Point};
    let point = Point::default();
    // (kind, what the middle frame shows at a quarter, at three quarters, in
    // the centre and in a corner), red for the old scene, green for the new.
    type Check = fn((u8, u8)) -> bool;
    let cases: Vec<(Kind, [Check; 4])> = vec![
        (Kind::Wipe { direction: Direction::Left }, [is_red, is_green, |_| true, is_red]),
        (Kind::Wipe { direction: Direction::Right }, [is_green, is_red, |_| true, is_green]),
        (Kind::Slide { direction: Direction::Left }, [is_red, is_green, |_| true, is_red]),
        (Kind::Push { direction: Direction::Right }, [is_green, is_red, |_| true, is_green]),
        (Kind::Zoom { point }, [|_| true, |_| true, is_green, is_red]),
        (Kind::ZoomOut { point }, [|_| true, |_| true, is_red, is_green]),
        (Kind::Box { point }, [|_| true, |_| true, is_green, is_red]),
        (Kind::Dip { colour: 0xffff_ffff }, [|p| p.0 > 200, |p| p.0 > 200, |p| p.0 > 200, |p| p.0 > 200]),
        (Kind::Dip { colour: 0xff00_0000 }, [|p| p.0 < 40, |p| p.0 < 40, |p| p.0 < 40, |p| p.0 < 40]),
    ];
    let mut mix = coloured(&["red", "green"]).await;
    let (w, h) = (mix.canvas.width, mix.canvas.height);
    let frame = mix.canvas.frame_duration().nseconds();
    let points = vec![(w / 4, h / 2), (3 * w / 4, h / 2), (w / 2, h / 2), (6, 6)];
    for (kind, checks) in cases {
        let name = kind.name().to_string();
        let (tap, window, largest) = cross(&mut mix, spec(kind), points.clone()).await;
        // Two frames, times the slack a loaded machine declares: a frame
        // missed there is the machine, and the gap is printed either way.
        let allowed = (frame as f64 * 2.0 * crate::plugin::harness::timing_slack()) as u64;
        println!("{name}: largest interval {:.1} ms", largest as f64 / 1e6);
        assert!(largest <= allowed, "{name}: the largest interval was {largest} ns against a frame of {frame}");
        let mid = tap.at(window.0 + (window.1 - window.0) / 2);
        // The window a tenth at a time, for the message: whether the new
        // scene came late or the curve went wrong reads off it at once.
        let tenths: Vec<_> = (0..=10).map(|t| tap.at(window.0 + (window.1 - window.0) * t / 10)).collect();
        let tenths = format!("{tenths:?}, the new scene first drawn {:?} ms in", tap.first_green_after(window.0));
        for (i, check) in checks.iter().enumerate() {
            assert!(check(mid[i]), "{name}: point {:?} half way through showed {:?}; by tenths {tenths}", points[i], mid[i]);
        }
        landed_on_green(&mix);
        let after = tap.at(window.1 + frame * 4);
        assert!(after.iter().all(|p| is_green(*p)), "{name}: after landing the canvas showed {after:?}");
    }
    mix.shutdown();
}

/// A wipe trims the picture rather than squashing it: the incoming pad's
/// own caps narrow as the edge crosses, which only a crop does.
#[tokio::test(flavor = "multi_thread")]
async fn a_wipe_is_a_crop_on_the_slot_and_not_a_squash() {
    let mut mix = coloured(&["red", "green"]).await;
    let canvas = mix.canvas.clone();
    mix.take_scene(scene("a", vec![full(&canvas, "red")]), None).expect("the first scene");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let spec = TransitionSpec::new(Kind::Wipe { direction: Default::default() }, 1000);
    mix.take_scene_over(scene("b", vec![full(&canvas, "green")]), None, None, Some(spec)).expect("a wipe");
    let pad = mix
        .pool
        .slots()
        .iter()
        .find(|s| s.source().map(String::as_str) == Some("green") && s.pad().control_binding("width").is_some())
        .map(|s| s.pad().clone())
        .expect("the incoming pad is driven");
    // The slot keeps up while its crop changes size every frame: a
    // renegotiation that waited on the compositor let through five buffers
    // in half a second.
    let index = mix.pool.slots().iter().find(|s| s.pad() == &pad).map(|s| s.index).expect("its slot");
    let crop = mix.program.by_name(&format!("slot-crop-{index}")).expect("the slot's crop");
    let seen = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counter = seen.clone();
    crop.static_pad("src").expect("a src pad").add_probe(gst::PadProbeType::BUFFER, move |_p, _i| {
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    let through = seen.load(std::sync::atomic::Ordering::Relaxed);
    assert!(through >= 10, "the slot let {through} frames through in half a second at 30 fps");
    let caps = pad.current_caps().expect("the pad has caps");
    let width: i32 = caps.structure(0).and_then(|s| s.get("width").ok()).expect("a width");
    assert!(width > 8 && width < canvas.width - 8, "half way through the picture is {width} wide of {}", canvas.width);
    tokio::time::sleep(Duration::from_millis(800)).await;
    mix.handle(Command::TransitionEnd { transition: mix.transition_id() }).expect("the end");
    landed_on_green(&mix);
    mix.shutdown();
}

/// The easing changes the frames between and nothing else.
#[test]
fn an_eased_fade_is_behind_a_linear_one_half_way_and_level_at_the_ends() {
    let _ = gst::init();
    let spec = |name: &str| {
        let request: godwinmix_protocol::requests::Transition =
            serde_json::from_value(serde_json::json!({"type": "fade", "params": {"easing": name}})).expect("a request");
        TransitionSpec::from(&request)
    };
    assert_eq!(spec("ease-in").easing, Easing::In);
    assert_eq!(spec("linear").easing, Easing::Linear);
    assert!(Easing::In.at(0.5) < Easing::Linear.at(0.5));
}

/// A request names the new kinds with their params, and a plain name gets the
/// defaults: left, the centre, black.
#[test]
fn a_request_names_each_new_kind_with_its_params() {
    use super::transition::{Direction, Point};
    let kind = |v: serde_json::Value| {
        let request: godwinmix_protocol::requests::Transition = serde_json::from_value(v).expect("a request");
        TransitionSpec::from(&request).kind
    };
    assert_eq!(kind(serde_json::json!("wipe")), Kind::Wipe { direction: Direction::Left });
    assert_eq!(
        kind(serde_json::json!({"type": "push", "params": {"direction": "up"}})),
        Kind::Push { direction: Direction::Up }
    );
    assert_eq!(kind(serde_json::json!({"type": "zoom", "params": {"x": 0.25, "y": 0.75}})), Kind::Zoom {
        point: Point { x: 250, y: 750 }
    });
    assert_eq!(kind(serde_json::json!({"type": "dip", "params": {"colour": "white"}})), Kind::Dip { colour: 0xffff_ffff });
    assert_eq!(kind(serde_json::json!("dip")), Kind::Dip { colour: 0xff00_0000 });
}

// ---------------------------------------------------------------------------
// Item motions
// ---------------------------------------------------------------------------

fn lower_third(canvas: &CanvasCaps, enter: Option<item::ItemMotion>, exit: Option<item::ItemMotion>) -> Placement {
    Placement {
        item: Some(crate::scene::id::Id::new()),
        xpos: 0,
        ypos: canvas.height * 2 / 3,
        width: canvas.width / 2,
        height: canvas.height / 4,
        motion: item::Motion { enter, exit },
        ..Placement::full_canvas("green".into(), canvas)
    }
}

fn slide(ms: u64) -> item::ItemMotion {
    item::ItemMotion { kind: item::ItemKind::Slide, duration_ms: ms, easing: Easing::Out, ..Default::default() }
}

fn with(background: &Placement, third: Option<&Placement>) -> ProgramScene {
    let mut placements = vec![background.clone()];
    placements.extend(third.cloned());
    scene("show", placements)
}

/// A lower third hidden on air slides out to the left and is gone; shown
/// again, it slides in and ends exactly where it was placed. The programme
/// never misses a frame on the way.
#[tokio::test(flavor = "multi_thread")]
async fn a_lower_third_slides_out_and_back_in_to_where_it_was_placed() {
    let mut mix = coloured(&["red", "green"]).await;
    let canvas = mix.canvas.clone();
    let frame = canvas.frame_duration().nseconds();
    let background = full(&canvas, "red");
    let third = lower_third(&canvas, Some(slide(400)), Some(slide(400)));
    mix.take_scene(with(&background, Some(&third)), None).expect("the show");
    let gaps = Arc::new(Gaps::default());
    gaps.watch(&mix.venc_tee.static_pad("sink").expect("the encoder tee has a sink pad"));
    gaps.wait_for(10).await;
    gaps.largest.store(0, std::sync::atomic::Ordering::Relaxed);
    let pad_of = |mix: &Mixer| {
        mix.pool.slots().iter().find(|s| s.item() == third.item).map(|s| s.pad().clone()).expect("the third's pad")
    };

    // Hidden: the same scene applied again without it.
    mix.take_scene_over(with(&background, None), None, None, None).expect("hide");
    let pad = pad_of(&mix);
    assert!(mix.transition_window().is_some(), "hiding an item with an exit plays it");
    tokio::time::sleep(Duration::from_millis(200)).await;
    let x: i32 = pad.property("xpos");
    assert!(x < 0, "part way out the third is off the left edge, at {x}");
    tokio::time::sleep(Duration::from_millis(400)).await;
    mix.handle(Command::TransitionEnd { transition: mix.transition_id() }).expect("the end");
    mix.apply_visibility(false);
    assert_eq!(pad.property::<f64>("alpha"), 0.0, "a hidden item is gone once it has left");
    assert_eq!(mix.pool.visible(), 1, "only the background is left");

    // Shown: back in, landing on its own placement.
    mix.take_scene_over(with(&background, Some(&third)), None, None, None).expect("show");
    assert!(mix.transition_window().is_some(), "showing an item with an enter plays it");
    let pad = pad_of(&mix);
    tokio::time::sleep(Duration::from_millis(150)).await;
    let x: i32 = pad.property("xpos");
    assert!(x < 0, "part way in it is still coming from the left, at {x}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    mix.handle(Command::TransitionEnd { transition: mix.transition_id() }).expect("the end");
    mix.apply_visibility(false);
    let placed = PadState { xpos: third.xpos, ypos: third.ypos, width: third.width, height: third.height, alpha: 1.0 };
    let slot = mix.pool.slots().iter().find(|s| s.item() == third.item).expect("the slot");
    assert_eq!(slot.state(), placed, "a shown item ends where it was placed");
    assert_eq!(mix.pool.visible(), 2);
    gaps.wait_for(4).await;
    let largest = gaps.largest.load(std::sync::atomic::Ordering::Relaxed);
    assert!(largest <= frame * 2, "the largest interval was {largest} ns against a frame of {frame}");
    mix.shutdown();
}

/// An item that enters with a wipe is revealed by its crop, and an item with
/// no motion is shown and hidden as the cut it always was.
#[tokio::test(flavor = "multi_thread")]
async fn an_item_wipes_in_by_its_crop_and_an_item_without_motion_cuts() {
    let mut mix = coloured(&["red", "green"]).await;
    let canvas = mix.canvas.clone();
    let background = full(&canvas, "red");
    let wipe = item::ItemMotion { kind: item::ItemKind::Wipe, duration_ms: 600, ..Default::default() };
    let third = lower_third(&canvas, Some(wipe), None);
    mix.take_scene(with(&background, None), None).expect("the show");
    tokio::time::sleep(Duration::from_millis(300)).await;
    mix.take_scene_over(with(&background, Some(&third)), None, None, None).expect("show");
    assert!(mix.transition_window().is_some());
    let crops = mix.pool.driven_crops();
    assert!(!crops.is_empty(), "a wiping item drives its slot's crop");
    tokio::time::sleep(Duration::from_millis(800)).await;
    mix.handle(Command::TransitionEnd { transition: mix.transition_id() }).expect("the end");
    assert!(mix.pool.driven_crops().is_empty(), "the crop is handed back");

    // Hidden with no exit: a cut, nothing held.
    mix.take_scene_over(with(&background, None), None, None, None).expect("hide");
    assert!(mix.transition_window().is_none(), "an item with no exit is hidden on the next frame");
    assert_eq!(mix.pool.visible(), 1);
    mix.shutdown();
}

/// With `on_take`, an item's entrance plays when its scene is cut to.
#[tokio::test(flavor = "multi_thread")]
async fn an_item_that_enters_on_take_plays_its_entrance_on_a_cut() {
    let mut mix = coloured(&["red", "green"]).await;
    let canvas = mix.canvas.clone();
    mix.take_scene(scene("before", vec![full(&canvas, "red")]), None).expect("the first scene");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let enter = item::ItemMotion { on_take: true, ..slide(300) };
    let third = lower_third(&canvas, Some(enter), None);
    let background = full(&canvas, "red");
    mix.take_scene_over(scene("after", vec![background, third.clone()]), None, None, None).expect("the cut");
    assert!(mix.transition_window().is_some(), "a cut with an entrance on take still plays it");
    tokio::time::sleep(Duration::from_millis(500)).await;
    mix.handle(Command::TransitionEnd { transition: mix.transition_id() }).expect("the end");
    mix.apply_visibility(false);
    let pad = mix.pool.slots().iter().find(|s| s.item() == third.item && s.showing()).expect("the third is drawn");
    assert_eq!(pad.state().xpos, third.xpos, "and lands where it was placed");
    assert_eq!(mix.pool.visible(), 2, "the old scene is gone, the new one whole");
    mix.shutdown();
}

// ---------------------------------------------------------------------------
// What each costs
// ---------------------------------------------------------------------------

/// The programme's CPU while each transition runs back to back, against the
/// same takes as cuts, at 1280x720 on the software compositor.
///
/// Each run is ten takes, 600 ms apart, between two full canvas test
/// patterns; a transition is 500 ms of each 600, so the window is in a
/// transition most of the time it is measured. Ignored by default; run it
/// with `cargo test -p godwinmix-core --lib -- --ignored --nocapture
/// what_each_transition_costs`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a CPU measurement that takes about a minute"]
async fn what_each_transition_costs_against_a_cut() {
    use super::tests::cpu_seconds;
    use super::transition::{Direction, Point};
    let mut cfg = super::tests::programme_config(crate::config::Accel::Software);
    cfg.canvas.width = 1280;
    cfg.canvas.height = 720;
    let mut mix = with_sources_cfg(&["cam1", "cam2"], cfg).await;
    let canvas = mix.canvas.clone();
    let frame = canvas.frame_duration().nseconds();
    let gaps = Arc::new(Gaps::default());
    gaps.watch(&mix.venc_tee.static_pad("sink").expect("the encoder tee has a sink pad"));
    mix.take_scene(scene("a", vec![full(&canvas, "cam1")]), None).expect("the first scene");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let point = Point::default();
    let kinds: Vec<Option<Kind>> = vec![
        None,
        Some(Kind::Fade),
        Some(Kind::Wipe { direction: Direction::Left }),
        Some(Kind::Box { point }),
        Some(Kind::Slide { direction: Direction::Left }),
        Some(Kind::Push { direction: Direction::Left }),
        Some(Kind::Zoom { point }),
        Some(Kind::ZoomOut { point }),
        Some(Kind::Dip { colour: 0xffff_ffff }),
    ];
    let mut cut = 0.0;
    for kind in kinds {
        let name = kind.as_ref().map(|k| k.name().to_string()).unwrap_or_else(|| "cut".into());
        gaps.largest.store(0, std::sync::atomic::Ordering::Relaxed);
        let start = cpu_seconds();
        let at = std::time::Instant::now();
        for i in 0..10 {
            let (id, source) = if i % 2 == 0 { ("b", "cam2") } else { ("a", "cam1") };
            let spec = kind.clone().map(|k| TransitionSpec::new(k, 500));
            mix.take_scene_over(scene(id, vec![full(&canvas, source)]), None, None, spec).expect("a take");
            tokio::time::sleep(Duration::from_millis(550)).await;
            if mix.transition_window().is_some() {
                mix.handle(Command::TransitionEnd { transition: mix.transition_id() }).expect("the end");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let cores = (cpu_seconds() - start) / at.elapsed().as_secs_f64();
        if kind.is_none() {
            cut = cores;
        }
        let largest = gaps.largest.load(std::sync::atomic::Ordering::Relaxed);
        println!(
            "transition cost: {name:<9} {cores:.3} cores, {:+6.1} percent against a cut, largest interval {:.1} ms (a frame is {:.1})",
            if cut > 0.0 { (cores - cut) / cut * 100.0 } else { 0.0 },
            largest as f64 / 1e6,
            frame as f64 / 1e6
        );
    }
    mix.shutdown();
}
