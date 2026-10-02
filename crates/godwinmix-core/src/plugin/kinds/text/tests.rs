use super::*;
use crate::config::SourceConfig;

fn params(toml_text: &str) -> Params {
    toml::from_str(toml_text).unwrap()
}

#[test]
fn a_text_address_is_claimed_and_its_words_are_the_text() {
    let kind = |uri: &str| crate::plugin::source::resolve(uri).map(|p| p.manifest.provide_id());
    assert_eq!(kind("text:Hello").as_deref(), Some("text/source"));
    let p = validate(&params("uri = \"text:Breaking news\"")).unwrap();
    assert_eq!(p.text, "Breaking news");
    let p = validate(&params("uri = \"text:x\"\ntext = \"Given\"")).unwrap();
    assert_eq!(p.text, "Given", "params.text wins over the address");
}

#[test]
fn a_bad_param_names_itself_and_what_would_do() {
    let e = validate(&params("colour = \"#fff\"")).unwrap_err().to_string();
    assert!(e.contains("\"colour\"") && e.contains("color"), "{e}");
    let e = validate(&params("color = \"white\"")).unwrap_err().to_string();
    assert!(e.contains("params.color") && e.contains("#rrggbb"), "{e}");
    let e = validate(&params("weight = 1000")).unwrap_err().to_string();
    assert!(e.contains("100 to 900"), "{e}");
}

#[test]
fn words_render_once_into_a_box_with_room_round_them() {
    let _ = gstreamer::init();
    let p = validate(&params("text = \"Ada Lovelace\\nAnalyst\"\nsize = 40\npadding = 20\nbackground = \"#102030\"")).unwrap();
    let r = p.render(None).unwrap();
    let pic = r.picture.expect("a picture");
    assert!(pic.width > 200 && pic.height > 80, "{}x{}", pic.width, pic.height);
    assert_eq!(pic.natural, (pic.width, pic.height));
    let map = pic.buffer.map_readable().unwrap();
    // The middle of the top edge is inside the box: opaque. Somewhere in the
    // middle the letters are lighter than the dark box.
    assert_eq!(map[(pic.width as usize / 2) * 4], 255);
    let brightest = map.chunks_exact(4).map(|px| px[1]).max().unwrap();
    assert!(brightest > 200, "no letters drawn, brightest luma {brightest}");
}

#[test]
fn drawn_larger_it_renders_larger_rather_than_stretching() {
    let _ = gstreamer::init();
    let p = validate(&params("text = \"Score 2 1\"\nsize = 30")).unwrap();
    let small = p.render(None).unwrap().picture.unwrap();
    let big = p.render(Some((small.width * 2, small.height * 2))).unwrap().picture.unwrap();
    assert_eq!((big.width, big.height), (small.width * 2, small.height * 2));
    assert_eq!(big.natural, small.natural, "the shape it keeps is its own size");
}

#[test]
fn a_text_passes_the_checks_every_kind_passes() {
    let _ = gstreamer::init();
    let cfg = SourceConfig::bare("harness-text", "text:Hello");
    let report = crate::plugin::harness::check_source(&cfg, false).expect("the harness runs");
    report.lines().iter().for_each(|l| println!("{l}"));
    report.into_result().expect("text/source is conformant");
}
