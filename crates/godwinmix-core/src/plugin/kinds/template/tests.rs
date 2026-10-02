use super::*;
use crate::config::SourceConfig;

fn params(toml_text: &str) -> Params {
    toml::from_str(toml_text).unwrap()
}

#[test]
fn a_template_address_is_claimed_and_names_its_template() {
    let kind = |uri: &str| crate::plugin::source::resolve(uri).map(|p| p.manifest.provide_id());
    assert_eq!(kind("template:score-bug").as_deref(), Some("template/source"));
    let p = validate(&params("uri = \"template:score-bug\"\n[fields]\nscore_home = 2\nhome = \"ARS\"")).unwrap();
    assert_eq!(p.template.info.name, "score-bug");
    assert_eq!(p.values.get("score_home").map(String::as_str), Some("2"), "a number is taken as its digits");
}

#[test]
fn what_is_wrong_says_what_would_do() {
    let e = validate(&params("uri = \"template:nothing-here\"")).unwrap_err().to_string();
    assert!(e.contains("news-lower-third") && e.contains("template.list"), "{e}");
    let e = validate(&params("uri = \"template:logo-bug\"\ncolour = 1")).unwrap_err().to_string();
    assert!(e.contains("\"colour\"") && e.contains("fields"), "{e}");
    let e = validate(&params("uri = \"template:logo-bug\"\n[fields]\nheadline = \"x\"")).unwrap_err().to_string();
    assert!(e.contains("station, tag"), "{e}");
}

#[test]
fn a_template_passes_the_checks_every_kind_passes() {
    let _ = gstreamer::init();
    if !crate::probe::exists("rsvgdec") {
        println!("skipping: no rsvgdec");
        return;
    }
    let cfg = SourceConfig::bare("harness-template", "template:logo-bug");
    let report = crate::plugin::harness::check_source(&cfg, false).expect("the harness runs");
    report.lines().iter().for_each(|l| println!("{l}"));
    report.into_result().expect("template/source is conformant");
}
