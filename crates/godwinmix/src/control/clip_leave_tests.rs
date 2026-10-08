use super::*;

/// Two scenes: `Opener` shows the clip, `Studio` shows the cameras.
fn draws(scene: &str) -> Vec<String> {
    match scene {
        "Opener" => vec!["intro".into(), "logo".into()],
        "Studio" => vec!["cam1".into(), "cam2".into()],
        "Also opener" => vec!["intro".into()],
        _ => Vec::new(),
    }
}

fn scene(s: &str) -> OnAir {
    OnAir::Scene(s.into())
}

#[test]
fn the_armed_scene_comes_first() {
    let to = choose("intro", &scene("Opener"), Some("Studio"), Some(&scene("Pre-show")), draws);
    assert_eq!(to, Some(json!({ "scene": "Studio" })));
}

#[test]
fn with_nothing_armed_it_goes_back_to_what_was_on_before() {
    let to = choose("intro", &scene("Opener"), None, Some(&scene("Studio")), draws);
    assert_eq!(to, Some(json!({ "scene": "Studio" })));
    let to = choose("intro", &OnAir::Source("intro".into()), None, Some(&OnAir::Source("cam1".into())), draws);
    assert_eq!(to, Some(json!({ "source": "cam1" })));
}

/// Arming the scene on air, or one that also shows the clip, says nothing
/// about where to go next; what was on before still does.
#[test]
fn an_armed_scene_that_would_not_leave_the_clip_is_passed_over() {
    let to = choose("intro", &scene("Opener"), Some("Opener"), Some(&scene("Studio")), draws);
    assert_eq!(to, Some(json!({ "scene": "Studio" })));
    let to = choose("intro", &scene("Opener"), Some("Also opener"), Some(&scene("Studio")), draws);
    assert_eq!(to, Some(json!({ "scene": "Studio" })));
}

/// The programme is never cut to black for a clip, and never taken to a
/// scene that would show the same held frame.
#[test]
fn with_nowhere_to_go_it_stays() {
    assert_eq!(choose("intro", &scene("Opener"), None, None, draws), None);
    assert_eq!(choose("intro", &scene("Opener"), None, Some(&OnAir::Black), draws), None);
    assert_eq!(choose("intro", &scene("Opener"), None, Some(&scene("Also opener")), draws), None);
    assert_eq!(choose("intro", &OnAir::Source("intro".into()), None, Some(&OnAir::Source("intro".into())), draws), None);
}

#[test]
fn a_take_names_what_is_on_air() {
    assert_eq!(OnAir::from_take(Some("cam1".into()), None), OnAir::Source("cam1".into()));
    assert_eq!(OnAir::from_take(None, Some("Studio".into())), scene("Studio"));
    assert_eq!(OnAir::from_take(None, None), OnAir::Black);
}
