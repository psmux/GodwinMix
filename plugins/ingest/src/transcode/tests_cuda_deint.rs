//! The card's deinterlacer (`cuda_deint`), on a machine with an NVIDIA card
//! and GStreamer's CUDA elements; elsewhere each test says why it did
//! nothing and passes. The same moving bar as the CPU test in
//! `tests_picture`, and a still picture with soft detail that must come
//! through untouched.

use serde_json::json;

use super::tests_picture::{comb, moving, node, run_with, Run};

const STILL: &str = "video/x-raw,format=I420,width=704,height=480,framerate=30000/1001,pixel-aspect-ratio=1/1,interlace-mode=mixed";
const MIXED: &str = "video/x-raw,format=I420,width=320,height=240,framerate=30000/1001,pixel-aspect-ratio=1/1,interlace-mode=mixed";

fn card() -> bool {
    gmx_netkit::init().unwrap();
    let ok = super::cuda_deint::available() && gstreamer::ElementFactory::find("cudaupload").is_some();
    if !ok {
        eprintln!("no CUDA here: the card's deinterlacer is not tested");
    }
    ok
}

fn on_card(extra: serde_json::Value) -> super::spec::NodeSpec {
    let mut n = node(json!({"width": 320, "height": 240, "memory": "cuda"}));
    for (k, v) in extra.as_object().unwrap() {
        n.raw[k] = v.clone();
    }
    n
}

#[test]
fn the_card_takes_the_comb_off_a_moving_interlaced_picture() {
    if !card() {
        return;
    }
    let fields = || [Run { caps: MIXED, size: (320, 240), count: 8, fields: true, luma: &moving }];
    let woven = run_with(&on_card(json!({"deinterlace": "off"})), &fields(), true);
    let before = comb(&woven, woven.frames.len() - 1);
    assert!(before > 10_000, "woven on the card, the bar's edges comb: {before}");
    for (mode, method) in [("on", "yadif"), ("auto", "yadif"), ("on", "linear")] {
        let out = run_with(&on_card(json!({"deinterlace": mode, "method": method})), &fields(), true);
        let after = comb(&out, out.frames.len() - 1);
        assert!(after * 3 < before, "deinterlace {mode} with {method} on the card leaves a comb: {after} against {before} woven");
    }
    let frames = [Run { caps: MIXED, size: (320, 240), count: 8, fields: false, luma: &moving }];
    let kept = run_with(&on_card(json!({"deinterlace": "auto"})), &frames, true);
    assert_eq!(comb(&kept, kept.frames.len() - 1), before, "auto deinterlaced on the card a picture not flagged as fields");
}

/// A still picture of soft vertical detail, as a broadcast camera filters it
/// for interlace: `yadif` on the card keeps it woven, line for line.
#[test]
fn the_card_keeps_still_detail_woven() {
    if !card() {
        return;
    }
    let soft = |_: usize, x: usize, y: usize| (60 + (x % 50) + 2 * (y % 40)) as u8;
    let still = [Run { caps: STILL, size: (704, 480), count: 4, fields: true, luma: &soft }];
    let out = run_with(&on_card(json!({"deinterlace": "on", "method": "yadif", "width": 704, "height": 480})), &still, true);
    let (w, _, l) = &out.frames[out.frames.len() - 1];
    let worst = (100..380).flat_map(|y| (100..600).map(move |x| (x, y))).map(|(x, y)| l[y * w + x].abs_diff(soft(0, x, y))).max().unwrap();
    assert!(worst <= 3, "still detail changed by {worst} on the card");
}
