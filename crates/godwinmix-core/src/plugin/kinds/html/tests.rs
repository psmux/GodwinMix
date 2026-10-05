use super::params::{validate, Cue};
use crate::config::Params;
use serde_json::Value;

fn params(uri: &str, extra: &[(&str, toml::Value)]) -> Params {
    let mut p = Params::new();
    p.insert("uri".into(), toml::Value::String(uri.into()));
    for (k, v) in extra {
        p.insert((*k).into(), v.clone());
    }
    p
}

fn fields(pairs: &[(&str, &str)]) -> toml::Value {
    toml::Value::Table(pairs.iter().map(|(k, v)| (k.to_string(), toml::Value::String(v.to_string()))).collect())
}

#[test]
fn an_unknown_design_or_field_says_which_there_are() {
    let e = validate(&params("html:nothing-here", &[])).unwrap_err().to_string();
    assert!(e.contains("lower-third-glass"), "{e}");
    let e = validate(&params("html:lower-third-glass", &[("fields", fields(&[("headline", "x")]))])).unwrap_err().to_string();
    assert!(e.contains("name") && e.contains("title"), "{e}");
    let e = validate(&params("html:lower-third-glass", &[("cue", toml::Value::String("maybe".into()))])).unwrap_err().to_string();
    assert!(e.contains("auto"), "{e}");
}

#[test]
fn the_state_carries_every_field_and_follows_the_programme_unless_held() {
    let p = validate(&params("html:lower-third-glass", &[("fields", fields(&[("name", "Ada")]))])).unwrap();
    assert_eq!(p.cue, Cue::Auto);
    let off: Value = serde_json::from_str(&p.state(false)).unwrap();
    assert_eq!(off["cue"], "out");
    assert_eq!(off["fields"]["name"], "Ada");
    assert_eq!(off["fields"]["title"], "Head of Engine Research", "a field not set shows its default");
    let on: Value = serde_json::from_str(&p.state(true)).unwrap();
    assert_eq!(on["cue"], "in");
    let held = validate(&params("html:lower-third-glass", &[("cue", toml::Value::String("in".into()))])).unwrap();
    let state: Value = serde_json::from_str(&held.state(false)).unwrap();
    assert_eq!(state["cue"], "in", "held in shows off air too");
}

#[test]
fn a_design_asks_for_its_own_frame_rate_and_params_can_lower_it() {
    let p = validate(&params("html:background-gradient", &[])).unwrap();
    assert_eq!((p.fps, p.template.fps), (0, Some(20)));
    let e = validate(&params("html:background-gradient", &[("fps", toml::Value::Integer(500))])).unwrap_err().to_string();
    assert!(e.contains("1 to 60"), "{e}");
}
