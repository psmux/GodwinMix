//! Checking a `config.set` and writing it: every value or none.
//!
//! Each value is checked against its key on its own first (the type the struct
//! derives, the range and the choices in `keys.rs`), and then the whole file
//! as it would be is loaded the way a start loads it, which catches what one
//! value cannot show on its own: an odd canvas width, a snapshot limit below
//! its default. Only a file that loads is written, because a half written
//! config that does not parse is a mixer that will not start.

use std::path::Path;

use serde_json::{json, Map, Value};

use super::keys::{self, Key};
use super::{edit, Config};

/// Why a value was not taken, and what a client can do about it.
#[derive(Debug, Clone)]
pub struct Refused {
    pub key: String,
    pub message: String,
    /// `key` plus whatever would have worked: `minimum`, `maximum`,
    /// `choices`, `expected`, `owner` or `valid`.
    pub data: Map<String, Value>,
}

impl Refused {
    fn new(key: &str, message: String) -> Self {
        let mut data = Map::new();
        data.insert("key".into(), key.into());
        Self { key: key.to_string(), message, data }
    }
    fn with(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.data.insert(name.into(), value.into());
        self
    }
}

/// One key's change: `Some` writes the value, `None` takes the key out of
/// the file so the built in default applies.
pub type Change = (String, Option<toml::Value>);

/// The row for a key `config.set` takes, or the refusal that says where to go.
pub fn settable(key: &str) -> Result<&'static Key, Refused> {
    if let Some(found) = keys::find(key) {
        return Ok(found);
    }
    Err(match keys::owner_of(key) {
        Some(owner) => Refused::new(
            key,
            format!("`{key}` is not a setting config.set changes: it belongs to {owner}. Use that instead."),
        )
        .with("owner", owner),
        None => Refused::new(
            key,
            format!("there is no setting `{key}`. config.schema lists every one; send one of those."),
        )
        .with("valid", keys::KEYS.iter().map(|k| k.key).collect::<Vec<_>>()),
    })
}

/// Check one value and turn it into what the file will hold.
pub fn check(key: &str, value: &Value) -> Result<Change, Refused> {
    let row = settable(key)?;
    if value.is_null() || (row.secret && value.as_str() == Some("")) {
        return Ok((key.to_string(), None));
    }
    let derived = super::schema::derived();
    let kind = derived
        .get(key)
        .and_then(|p| p.get("type"))
        .map(expected_type)
        .unwrap_or("string");
    if !fits(kind, value) {
        return Err(refuse_type(row, kind, value));
    }
    in_range(row, value)?;
    let toml = toml::Value::try_from(value).map_err(|e| {
        Refused::new(key, format!("`{key}` has no TOML form ({e}). Send a plain value."))
    })?;
    Ok((key.to_string(), Some(toml)))
}

/// The one type a derived `type` names, `["integer", "null"]` included.
fn expected_type(t: &Value) -> &'static str {
    let names: Vec<&str> = match t {
        Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
        other => other.as_str().into_iter().collect(),
    };
    match names.into_iter().find(|n| *n != "null") {
        Some("boolean") => "boolean",
        Some("integer") => "integer",
        Some("number") => "number",
        Some("array") => "array",
        Some("object") => "object",
        _ => "string",
    }
}

fn fits(kind: &str, value: &Value) -> bool {
    match kind {
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "array" => value.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
        "object" => value.as_object().is_some_and(|o| o.values().all(Value::is_string)),
        _ => value.is_string(),
    }
}

fn refuse_type(row: &Key, kind: &str, value: &Value) -> Refused {
    let wanted = match kind {
        "boolean" => "true or false",
        "integer" => "a whole number",
        "number" => "a number",
        "array" => "a list of text",
        "object" => "a table of names to text",
        _ => "text",
    };
    let r = Refused::new(
        row.key,
        format!("`{}` takes {wanted}, and {value} is not that. Send {wanted}.", row.key),
    )
    .with("expected", kind);
    bounds(r, row)
}

fn bounds(mut r: Refused, row: &Key) -> Refused {
    if let Some(min) = row.min {
        r = r.with("minimum", min);
    }
    if let Some(max) = row.max {
        r = r.with("maximum", max);
    }
    if !row.choices.is_empty() {
        r = r.with("choices", row.choices.to_vec());
    }
    r
}

fn in_range(row: &Key, value: &Value) -> Result<(), Refused> {
    if let (Some(n), Some(min), Some(max)) = (value.as_i64(), row.min, row.max) {
        if n < min || n > max {
            let unit = row.unit.map(|u| format!(" {u}")).unwrap_or_default();
            return Err(bounds(
                Refused::new(
                    row.key,
                    format!("`{}` must be between {min} and {max}{unit}, and {n} is not. Send a value in that range.", row.key),
                ),
                row,
            ));
        }
    }
    if let (Some(word), false) = (value.as_str(), row.choices.is_empty()) {
        if !row.choices.contains(&word) {
            return Err(bounds(
                Refused::new(
                    row.key,
                    format!("`{}` is one of {}, and \"{word}\" is not. Send one of those.", row.key, row.choices.join(", ")),
                ),
                row,
            ));
        }
    }
    if value.as_u64().is_some_and(|n| n > i64::MAX as u64) {
        return Err(bounds(Refused::new(row.key, format!("`{}` is too large for the config file. Send a smaller number.", row.key)), row));
    }
    Ok(())
}

/// Every write goes through this, so two `config.set` calls cannot interleave
/// a read and a write of the same file.
static WRITING: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Apply `changes` to the file at `path` and return the config it now holds.
///
/// The file is loaded as it would be after the change first; when that fails
/// nothing is written and the answer is the refusal. `dry_run` checks and
/// writes nothing.
pub fn write(path: &Path, changes: &[Change], dry_run: bool) -> anyhow::Result<Result<Config, Refused>> {
    let _one_at_a_time = WRITING.lock();
    let text = std::fs::read_to_string(path)?;
    let mut doc: toml_edit::DocumentMut = text.parse()?;
    for (key, value) in changes {
        match value {
            Some(v) => edit::set(&mut doc, key, v)?,
            None => {
                edit::remove(&mut doc, key)?;
            }
        }
    }
    let candidate = doc.to_string();
    let loaded = match Config::from_toml(&candidate, &path.display().to_string()) {
        Ok(cfg) => cfg,
        Err(e) => {
            let key = changes.first().map(|(k, _)| k.as_str()).unwrap_or_default();
            let reason = format!("{e:#}");
            return Ok(Err(Refused::new(
                key,
                format!("nothing was written: with this change {} would not load ({reason}). Send values that fit together.", path.display()),
            )
            .with("reason", reason)
            .with("keys", json!(changes.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>()))));
        }
    };
    if !dry_run && candidate != text {
        edit::write_atomic(path, candidate.as_bytes())?;
    }
    Ok(Ok(loaded))
}
