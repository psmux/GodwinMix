//! The settable key table held to the structs it describes.

use serde_json::{json, Value};

use super::keys::{self, Applies, KEYS};
use super::schema::{self, lookup};
use super::settable::{check, write};
use super::Config;

#[test]
fn every_field_of_every_section_has_a_row_and_every_row_a_field() {
    let derived: Vec<String> = schema::derived().keys().cloned().collect();
    let rows: Vec<String> = KEYS.iter().map(|k| k.key.to_string()).collect();
    let missing: Vec<&String> = derived.iter().filter(|k| !rows.contains(k)).collect();
    let stale: Vec<&String> = rows.iter().filter(|k| !derived.contains(k)).collect();
    assert!(missing.is_empty(), "fields with no row in keys.rs: {missing:?}");
    assert!(stale.is_empty(), "rows in keys.rs with no field behind them: {stale:?}");
}

#[test]
fn every_property_is_something_a_form_can_draw() {
    let schema = schema::schema();
    let props = schema["properties"].as_object().unwrap();
    assert_eq!(props.len(), KEYS.len());
    for (key, p) in props {
        assert!(p["title"].is_string(), "{key} has no title");
        assert!(p["type"].is_string(), "{key} has no single type: {p}");
        assert!(p["x-gmx-group"].is_string(), "{key}");
        assert!(["live", "next_source", "restart"].contains(&p["x-gmx-applies"].as_str().unwrap()), "{key}");
    }
    let token = &props["control.token"];
    assert_eq!(token["format"], "secret");
    assert!(token.get("default").is_none(), "a secret has no default to show");
    assert_eq!(props["program.video_bitrate_kbps"]["default"], 6000);
    assert!(props["program.video_bitrate_kbps"]["description"].as_str().unwrap().contains("bitrate"));
    assert_eq!(props["hardware.encode"]["enum"][0], "auto");
    assert_eq!(props["safety.on_operator_silence.action"]["type"], "string");
}

/// A value for `key` that differs from its default and still loads.
fn another(key: &keys::Key, default: &Value, kind: &str) -> Value {
    if let Some(choice) = key.choices.iter().find(|c| Some(**c) != default.as_str()) {
        return json!(choice);
    }
    match (kind, default) {
        ("boolean", d) => json!(!d.as_bool().unwrap_or(false)),
        ("integer", d) => {
            let d = d.as_i64().unwrap_or(0);
            json!(if key.max.is_none_or(|m| d + 2 <= m) { d + 2 } else { d - 1 })
        }
        ("array", _) => json!(["x"]),
        ("object", _) => json!({ "A": "b" }),
        _ if key.key.ends_with(".action") => json!("hold"),
        _ => json!("elsewhere"),
    }
}

#[test]
fn take_live_copies_exactly_the_keys_that_do_not_wait_for_a_restart() {
    let defaults = schema::defaults();
    let derived = schema::derived();
    for key in KEYS {
        let kind = derived[key.key]["type"].clone();
        let kind = match &kind {
            Value::Array(t) => t.iter().find(|x| *x != "null").and_then(Value::as_str).unwrap_or("string").to_string(),
            other => other.as_str().unwrap_or("string").to_string(),
        };
        let value = another(key, lookup(&defaults, key.key).unwrap_or(&Value::Null), &kind);
        let (_, toml) = check(key.key, &value).unwrap_or_else(|e| panic!("{}: {}", key.key, e.message));
        let mut doc = toml_edit::DocumentMut::new();
        super::edit::set(&mut doc, key.key, &toml.unwrap()).unwrap();
        let from = Config::from_toml(&doc.to_string(), key.key).unwrap_or_else(|e| panic!("{}: {e:#}", key.key));
        let mut into: Config = toml::from_str("").unwrap();
        keys::take_live(&mut into, &from);
        let (a, b) = (schema::to_json(&into), schema::to_json(&from));
        let copied = lookup(&a, key.key) == lookup(&b, key.key);
        assert_eq!(copied, key.applies != Applies::Restart, "{} is {:?}", key.key, key.applies);
    }
}

#[test]
fn a_bad_value_is_refused_with_what_would_have_worked() {
    let e = check("program.video_bitrate_kbps", &json!(5)).unwrap_err();
    assert_eq!(e.data["minimum"], 100);
    assert!(e.message.contains("between 100 and 100000"), "{}", e.message);
    let e = check("multiview.enabled", &json!("yes")).unwrap_err();
    assert_eq!(e.data["expected"], "boolean");
    let e = check("hardware.encode", &json!("quantum")).unwrap_err();
    assert!(e.data["choices"].as_array().unwrap().contains(&json!("software")));
    let e = check("sources.0.uri", &json!("x")).unwrap_err();
    assert!(e.message.contains("source.add"), "{}", e.message);
    let e = check("canvas.depth", &json!(1)).unwrap_err();
    assert!(e.data["valid"].as_array().unwrap().contains(&json!("canvas.width")));
    assert_eq!(check("control.token", &json!("")).unwrap().1, None, "empty clears a secret");
}

#[test]
fn a_write_keeps_the_comments_and_one_that_would_not_load_writes_nothing() {
    let dir = std::env::temp_dir().join(format!("gmx-settable-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("godwinmix.toml");
    let mine = "# mine\n[canvas]\nwidth = 1280 # the hall projector\nheight = 720\n";
    std::fs::write(&path, mine).unwrap();

    let odd = check("canvas.width", &json!(1281)).unwrap();
    let refused = write(&path, &[odd], false).unwrap().unwrap_err();
    assert!(refused.message.contains("even"), "{}", refused.message);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), mine, "nothing was written");

    let wide = check("canvas.width", &json!(1920)).unwrap();
    let exec = check("security.allow_exec_sources", &json!(true)).unwrap();
    let cfg = write(&path, &[wide, exec], false).unwrap().unwrap();
    assert_eq!(cfg.canvas.width, 1920);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("# mine\n[canvas]\nwidth = 1920 # the hall projector\n"), "{text}");
    assert!(text.contains("[security]\nallow_exec_sources = true"), "{text}");

    let reset = check("canvas.width", &Value::Null).unwrap();
    write(&path, &[reset], false).unwrap().unwrap();
    assert!(!std::fs::read_to_string(&path).unwrap().contains("width"));
    std::fs::remove_dir_all(&dir).ok();
}
