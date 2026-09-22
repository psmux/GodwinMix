//! The settable keys as one flat JSON Schema, for a settings form.
//!
//! Type and description are derived from the config structs by `schemars`,
//! and the default is what an empty config file loads as, so neither can
//! disagree with the code. `keys.rs` adds what cannot be derived: the title,
//! the range, the unit and when a change applies. The result is shaped for
//! `ui/kits/schema`: one property per dotted key, grouped by `x-gmx-group`.

use serde_json::{json, Map, Value};

use super::keys::{Key, KEYS};
use super::Config;

/// The whole description: `{ "type": "object", "properties": { .. } }`.
pub fn schema() -> Value {
    let derived = derived();
    let defaults = defaults();
    let mut props = Map::new();
    for key in KEYS {
        let found = derived.get(key.key).cloned().unwrap_or_else(|| json!({}));
        props.insert(key.key.to_string(), property(key, found, lookup(&defaults, key.key)));
    }
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Mixer settings",
        "description": "Every setting config.set takes. x-gmx-applies says when a change \
                        takes effect: live, next_source or restart.",
        "type": "object",
        "properties": props,
    })
}

/// One key's property: the derived schema with the table's facts on top.
fn property(key: &Key, derived: Value, default: Option<&Value>) -> Value {
    let mut p = match derived {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    // `Option<T>` derives as `["integer", "null"]`. A form wants one type,
    // and an empty field is how it says null.
    if let Some(Value::Array(types)) = p.get("type").cloned() {
        let one = types.iter().find(|t| t.as_str() != Some("null")).cloned();
        p.insert("type".into(), one.unwrap_or(Value::Null));
        p.insert("x-gmx-nullable".into(), true.into());
    }
    p.insert("title".into(), key.title.into());
    if let Some(min) = key.min {
        p.insert("minimum".into(), min.into());
    }
    if let Some(max) = key.max {
        p.insert("maximum".into(), max.into());
    }
    if !key.choices.is_empty() {
        p.insert("enum".into(), key.choices.into());
    }
    if let Some(unit) = key.unit {
        p.insert("x-gmx-unit".into(), unit.into());
    }
    p.remove("default");
    if key.secret {
        p.insert("format".into(), crate::secrets::FORMAT.into());
        p.insert("writeOnly".into(), true.into());
    } else if let Some(default) = default.filter(|d| !d.is_null()) {
        p.insert("default".into(), default.clone());
    }
    p.insert("x-gmx-group".into(), key.section().into());
    p.insert("x-gmx-applies".into(), key.applies.as_str().into());
    Value::Object(p)
}

/// Every leaf of every section's derived schema, keyed by its dotted path.
pub fn derived() -> Map<String, Value> {
    let sections = [
        ("canvas", schema_of::<super::Canvas>()),
        ("program", schema_of::<super::ProgramConfig>()),
        ("multiview", schema_of::<super::MultiviewConfig>()),
        ("snapshot", schema_of::<super::SnapshotConfig>()),
        ("control", schema_of::<super::ControlConfig>()),
        ("hardware", schema_of::<super::HardwareConfig>()),
        ("media", schema_of::<super::MediaConfig>()),
        ("security", schema_of::<super::SecurityConfig>()),
        ("safety", schema_of::<crate::safety::SafetyConfig>()),
        ("browser", schema_of::<super::BrowserConfig>()),
        ("stall", schema_of::<super::StallConfig>()),
        ("nodes", schema_of::<super::NodesTable>()),
        ("plugins", schema_of::<super::PluginsTable>()),
    ];
    let mut out = Map::new();
    for (name, schema) in sections {
        let defs = schema.get("$defs").cloned().unwrap_or(Value::Null);
        flatten(name, &schema, &defs, &mut out);
    }
    out
}

fn schema_of<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Value::Null)
}

/// Walk `properties`, following a `$ref` into a nested struct so that
/// `[safety] on_operator_silence` becomes two keys, not one opaque object.
fn flatten(prefix: &str, node: &Value, defs: &Value, out: &mut Map<String, Value>) {
    let Some(props) = node.get("properties").and_then(Value::as_object) else { return };
    for (name, prop) in props {
        let path = format!("{prefix}.{name}");
        let target = resolve(prop, defs);
        if target.get("properties").is_some() {
            flatten(&path, &target, defs, out);
            continue;
        }
        out.insert(path, target);
    }
}

/// A property with its `$ref` replaced by what it points at, keeping the
/// property's own description over the target's.
fn resolve(prop: &Value, defs: &Value) -> Value {
    let Some(reference) = prop.get("$ref").and_then(Value::as_str) else { return prop.clone() };
    let name = reference.rsplit('/').next().unwrap_or_default();
    let mut target = defs.get(name).cloned().unwrap_or_else(|| json!({}));
    if let (Some(map), Some(own)) = (target.as_object_mut(), prop.as_object()) {
        for (k, v) in own {
            if k != "$ref" {
                map.insert(k.clone(), v.clone());
            }
        }
    }
    target
}

/// What an empty config file loads as, as JSON.
pub fn defaults() -> Value {
    toml::from_str::<Config>("").map(|cfg| to_json(&cfg)).unwrap_or(Value::Null)
}

/// A config as JSON with every settable key present.
///
/// `[plugins]` leaves its switches out of the file when they are at their
/// default, which is right for a file and wrong for a reader asking what the
/// value is, so they are put back here.
pub fn to_json(cfg: &Config) -> Value {
    let mut out = serde_json::to_value(cfg).unwrap_or(Value::Null);
    if let Some(root) = out.as_object_mut() {
        let plugins = root.entry("plugins").or_insert_with(|| json!({}));
        if let Some(p) = plugins.as_object_mut() {
            p.insert("allow_unsigned".into(), cfg.plugins.allow_unsigned.into());
            p.insert("allow_wasi".into(), json!(cfg.plugins.allow_wasi));
        }
    }
    out
}

/// The value at a dotted path in a JSON tree.
pub fn lookup<'a>(tree: &'a Value, dotted: &str) -> Option<&'a Value> {
    dotted.split('.').try_fold(tree, |at, step| at.get(step))
}
