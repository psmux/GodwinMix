use super::*;

fn output(text: &str) -> OutputConfig {
    toml::from_str(&format!("id = \"viewers\"\ntype = \"hls/output\"\n{text}")).unwrap()
}

fn rungs(cfg: &OutputConfig) -> Vec<RenditionRequest> {
    match choice(cfg).unwrap() {
        Some(RenditionChoice::Ladder(l)) => l.ladder,
        other => panic!("a ladder, not {other:?}"),
    }
}

#[test]
fn nothing_asked_is_the_programme_as_it_is() {
    assert_eq!(choice(&output("")).unwrap(), None);
    assert_eq!(choice(&output("rendition = { preset = \"copy\" }")).unwrap(), None);
}

#[test]
fn a_preset_by_any_of_its_names_is_a_ladder_with_keyframes_on_every_segment() {
    let four = rungs(&output("rendition = { preset = \"abr-ladder-4\" }"));
    assert_eq!(four.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["1080p", "720p", "480p", "360p"]);
    assert!(four.iter().all(|r| r.video.as_ref().unwrap().keyframe_ms == Some(2000)));
    let low = rungs(&output("params = { ladder = \"abr-ladder-3\", segment_ms = 1000, part_ms = 200 }"));
    assert_eq!(low.len(), 3);
    assert!(low.iter().all(|r| r.video.as_ref().unwrap().keyframe_ms == Some(1000) && r.container == Container::Hls));
    let e = choice(&output("params = { ladder = \"abr-ladder-9\" }")).unwrap_err().to_string();
    assert!(e.contains("abr-ladder-4"), "{e}");
}

#[test]
fn a_custom_ladder_of_rendition_requests() {
    let text = r#"
        [[params.rendition.ladder]]
        id = "540p"
        container = "hls"
        video = { codec = "h264", height = 540, bitrate_kbps = 2000, keyframe_ms = 4000 }
        [[params.rendition.ladder]]
        video = { height = 240 }
    "#;
    let r = rungs(&output(text));
    assert_eq!((r[0].id.as_str(), r[1].id.as_str()), ("540p", "240p"));
    assert_eq!(r[0].video.as_ref().unwrap().keyframe_ms, Some(2000), "the segment decides");
}

#[test]
fn a_custom_rung_is_refused_with_the_reason() {
    let bad = |video: &str, id: &str| {
        let text = format!("[[params.rendition.ladder]]\nid = \"{id}\"\nvideo = {video}\n");
        choice(&output(&text)).unwrap_err().to_string()
    };
    assert!(bad("{ codec = \"vp8\", height = 720 }", "720p").contains("h264"));
    assert!(bad("{ height = 99 }", "tiny").contains("144 to 2160"));
    assert!(bad("{ height = 720 }", "Big Rung").contains("slug"));
    assert!(bad("{ height = 720 }", "audio").contains("shares"));
}

#[test]
fn a_rung_is_named_by_what_follows_the_output() {
    assert_eq!(rung_slug("viewers", "viewers-720p", Some(720)), "720p");
    assert_eq!(rung_slug("viewers", "viewers", Some(480)), "480p");
    assert_eq!(rung_slug("viewers", "viewers", None), "programme");
}
