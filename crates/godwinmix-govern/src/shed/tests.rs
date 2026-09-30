use super::*;
use crate::testing::{gpu, profile, shape, x264};

fn held(id: u64, what: &str, cpu: u32, kind: Kind) -> Held {
    Held { id, what: what.into(), cost: Cost { cpu_millicores: cpu, ..Cost::default() }, kind, device: None, encode: None }
}

fn encoding(id: u64, what: &str, kind: Kind, preset: &str) -> Held {
    let s = shape(1920, 1080, 30);
    let cost = profile().encode_cost_at(&x264(), &s, Some(preset));
    Held { id, what: what.into(), cost, kind, device: None, encode: Some(Encode { slot: x264(), shape: s, preset: Some(preset.into()) }) }
}

fn a_busy_show() -> Vec<Held> {
    vec![
        encoding(1, "programme", Kind::Programme, "veryfast"),
        encoding(2, "youtube 1080p", Kind::Rung { index: 0 }, "veryfast"),
        encoding(3, "ladder 720p", Kind::Rung { index: 1 }, "veryfast"),
        encoding(4, "ladder 480p", Kind::Rung { index: 2 }, "veryfast"),
        held(5, "multiview", 300, Kind::Preview),
        held(6, "thumbnails", 100, Kind::Thumbnail),
    ]
}

#[test]
fn thumbnails_and_previews_go_first() {
    let steps = plan(&a_busy_show(), 350, &profile());
    let ids: Vec<u64> = steps.iter().map(|s| s.ticket).collect();
    assert_eq!(ids, [6, 5]);
    assert!(steps.iter().all(|s| s.action == ShedAction::Drop));
    assert!(steps[1].why.contains("multiview"), "{}", steps[1].why);
}

#[test]
fn then_the_lowest_rung_and_the_next_one_up() {
    let steps = plan(&a_busy_show(), 3000, &profile());
    let ids: Vec<u64> = steps.iter().map(|s| s.ticket).collect();
    assert_eq!(ids[..4], [6, 5, 4, 3]);
}

#[test]
fn then_faster_presets_and_never_the_programme_or_the_top_rung_dropped() {
    let steps = plan(&a_busy_show(), 100_000, &profile());
    for s in &steps {
        if s.ticket == 1 || s.ticket == 2 {
            assert!(matches!(s.action, ShedAction::LowerPreset { .. }), "{s:?}");
        }
    }
    let lowered: Vec<u64> =
        steps.iter().filter(|s| matches!(s.action, ShedAction::LowerPreset { .. })).map(|s| s.ticket).collect();
    // Rungs that were dropped are not lowered too; the top rung before the
    // programme.
    assert_eq!(lowered, [2, 1]);
    let ShedAction::LowerPreset { to, cost } = &steps.last().unwrap().action else { panic!() };
    assert_eq!(to, "superfast");
    assert!(cost.cpu_millicores < 2000);
}

#[test]
fn hardware_encodes_have_no_preset_to_lower() {
    let s = shape(1920, 1080, 30);
    let h = Held { id: 9, what: "gpu".into(), cost: profile().encode_cost(&gpu(), &s), kind: Kind::Programme, device: Some("gpu".into()), encode: Some(Encode { slot: gpu(), shape: s, preset: None }) };
    assert!(plan(&[h], 5000, &profile()).is_empty());
}

#[test]
fn nothing_is_shed_when_nothing_is_over() {
    assert!(plan(&a_busy_show(), 0, &profile()).is_empty());
}
