//! `config.get`, `config.set` and `core.restart`: the settings file, over the
//! API.
//!
//! The rule this exists to keep is the owner's: somebody using the GUI never
//! opens `godwinmix.toml`. Until now the only two things the API could change
//! in that file were a preset and a plugin's own settings, so the canvas, the
//! encoder bitrate, the safety limits, the mosaic, the stills, the stall
//! supervisor and the browser overlay rate were all file only, and a mixer
//! bought as an appliance needed a text editor and a terminal to tune.
//!
//! Three things make that safe enough to expose:
//!
//! * an allow list, `godwinmix_core::config::SETTABLE`. No token, no bind
//!   address, no output destination and no path to an executable is on it.
//! * the real type does the validating. A change is written into the document,
//!   the whole document is parsed back as a `Config`, and a key that makes that
//!   fail is refused by name with the parser's own reason. Nothing reaches the
//!   file that the mixer would not start on.
//! * the answer is redacted through the same function the support bundle uses,
//!   so a key added to the allow list later cannot leak a secret by accident.
//!
//! `core.restart` is the other half. `core.shutdown` goes down and stays down,
//! which is right for a mixer nobody is standing next to and useless for the
//! desktop app, where the shell is the core's parent and can start it again.
//! So the core exits with a distinct status the shell watches for, and a core
//! nobody supervises refuses and says who could.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::config::{self, Config, Kind, Setting, Timing};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set by `core.restart` and read by `main` on the way out. A flag rather than
/// a channel because the process is leaving either way and the only question
/// left is which status it leaves with.
static RESTART_WANTED: AtomicBool = AtomicBool::new(false);

/// True when `core.restart` was called and the process is on its way out
/// because of it. Read once, by `main_with_room`.
pub fn restart_wanted() -> bool {
    RESTART_WANTED.load(Ordering::SeqCst)
}

/// Whether something outside this process will start it again if it exits.
///
/// The desktop shell sets `GODWINMIX_SUPERVISED=1` before it spawns the core,
/// and a systemd unit with `Restart=always` sets it in the unit file. Nothing
/// infers it: a core has no way to look up and see its own parent, and a wrong
/// guess here is either a restart button that does nothing or a mixer that
/// leaves in the middle of a show.
pub fn supervised() -> bool {
    match config::env_var("SUPERVISED") {
        Some(value) => {
            let value = value.trim();
            !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
        }
        None => false,
    }
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "config.get",
            Scope::Read,
            "The settings this mixer is running on, key by key, with whether each one \
             applies at once or waits for a restart and where its value came from.",
            handler(get),
        )
        .result(schema_of::<ConfigDocument>)
        .tool(
            "get_config",
            Tier::Search,
            "The mixer's own settings: canvas, programme encoder, safety limits, mosaic, \
             stills, the stall supervisor and the browser overlay rate. Every key carries \
             its unit, its default, whether changing it needs a restart, and whether the \
             running mixer is already on the value in the file. Secrets are never in it. \
             Read it before `set_config` so you send the key names it uses.",
        ),
    );

    reg.register(
        MethodDef::new(
            "config.set",
            Scope::Admin,
            "Change settings in the config file. Validated against the real config type, \
             written keeping the file's comments, and applied now where the running mixer \
             has somewhere to put them.",
            handler(set),
        )
        .params(schema_of::<SetConfigRequest>)
        .result(schema_of::<SetConfigResult>)
        // Not because it takes anything off air, but because `dry_run` is
        // gated on this flag and a method that rewrites the operator's config
        // file should be one a confirm-required token has to confirm.
        .destructive()
        .tool(
            "set_config",
            Tier::Search,
            "Change one or more settings, by their dotted key: \
             `{\"changes\": {\"program.video_bitrate_kbps\": 4500}}`. Answers with what \
             is in force now and what is waiting for a restart. A key outside the allow \
             list, or a value the config type refuses, comes back naming the key and the \
             reason and nothing is written. Pass `dry_run` to see what it would do.",
        ),
    );

    reg.register(
        MethodDef::new(
            "core.restart",
            Scope::Admin,
            "Stop the mixer and let whatever started it start it again. Only on a core \
             something supervises; anywhere else it says so and names the next step.",
            handler(restart),
        )
        .result(schema_of::<RestartResult>)
        .destructive(),
    );
}

// --- config.get --------------------------------------------------------------

/// The settings, as a surface draws them.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConfigDocument {
    /// The file these settings were read from and are written back to. Empty
    /// on an embedded core, which has no file.
    pub path: String,
    /// True when something outside this process would start it again, so a
    /// surface may offer "apply and restart" rather than "restart it
    /// yourself".
    pub supervised: bool,
    /// True when the file holds a value the running mixer is not on yet.
    pub restart_pending: bool,
    /// The settable tables as one JSON document, for a client that wants the
    /// shape rather than the flat list.
    pub config: Value,
    /// Every settable key, in the order a form should draw them.
    pub keys: Vec<KeyInfo>,
}

/// One key, with everything a form needs to draw it and a person needs to
/// understand it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KeyInfo {
    /// The dotted path, for example `program.video_bitrate_kbps`.
    pub key: String,
    /// The table it belongs to, for grouping.
    pub table: String,
    /// What the running mixer is using.
    pub value: Value,
    /// What the file says, when that is not the same thing. Present only on a
    /// key that is waiting for a restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_value: Option<Value>,
    /// The shipped default, as the file would spell it.
    pub default: String,
    /// What the number is measured in, empty where it is not a measurement.
    pub unit: String,
    /// `integer`, `boolean` or `text`.
    pub kind: String,
    /// `hot` or `restart`.
    pub timing: String,
    /// `default` when the file does not mention it, `file` when it does and
    /// the mixer is on it, `pending restart` when the file has moved on.
    pub source: String,
    pub about: String,
}

async fn get(call: Call, _params: Value) -> Result<Value, RpcError> {
    let running = call.app.config();
    let path = call.app.config_path.as_path().to_path_buf();
    let on_file = std::fs::read_to_string(&path).unwrap_or_default();
    // What a restart would pick up: the file as the config type reads it, not
    // the raw text. A file that no longer parses leaves this `None` and every
    // key simply reports what is running.
    let from_file = Config::from_toml(&on_file, "the config file").ok();
    let raw: toml::Value = toml::from_str(&on_file).unwrap_or_else(|_| toml::Value::Table(toml::Table::new()));

    let running_doc = redacted(&running)?;
    let file_doc = from_file.as_ref().map(redacted).transpose()?;

    let mut keys = Vec::with_capacity(config::SETTABLE.len());
    let mut pending = false;
    for row in config::SETTABLE {
        let value = json_at(&running_doc, row.key);
        let file_value = file_doc.as_ref().map(|d| json_at(d, row.key));
        let written = path_present(&raw, row.key);
        let differs = file_value.as_ref().is_some_and(|f| *f != value);
        let source = match (written, differs) {
            (_, true) => "pending restart",
            (true, false) => "file",
            (false, false) => "default",
        };
        if differs {
            pending = true;
        }
        keys.push(KeyInfo {
            key: row.key.to_string(),
            table: row.key.split('.').next().unwrap_or_default().to_string(),
            value,
            file_value: differs.then(|| file_value.unwrap_or(Value::Null)),
            default: row.default.to_string(),
            unit: row.unit.to_string(),
            kind: row.kind.as_str().to_string(),
            timing: row.timing.as_str().to_string(),
            source: source.to_string(),
            about: row.about.to_string(),
        });
    }

    body(ConfigDocument {
        path: path.display().to_string(),
        supervised: supervised(),
        restart_pending: pending,
        config: settable_tables(&running_doc),
        keys,
    })
}

/// A configuration with every secret taken out.
///
/// The same function the support bundle uses, run over the same text: a value
/// under a key that names a credential is replaced, and so is the tail of any
/// URL, because on every streaming service the last path segment is the stream
/// key. Only the settable tables reach a caller anyway, and none of them holds
/// a secret today. This is here so that stays true when somebody adds a row to
/// the allow list without thinking about it.
fn redacted(cfg: &Config) -> Result<toml::Value, RpcError> {
    let text = toml::to_string(cfg)
        .map_err(|e| RpcError::internal(format!("writing the config out to redact it: {e}")))?;
    let clean = crate::cli::bundle::redact_config(&text);
    toml::from_str(&clean)
        .map_err(|e| RpcError::internal(format!("reading the redacted config back: {e}")))
}

/// Just the tables the allow list names, as JSON.
fn settable_tables(doc: &toml::Value) -> Value {
    let mut out = Map::new();
    let mut tables: Vec<&str> =
        config::SETTABLE.iter().filter_map(|r| r.key.split('.').next()).collect();
    tables.dedup();
    for name in tables {
        if let Some(found) = doc.get(name) {
            if let Ok(value) = serde_json::to_value(found) {
                out.insert(name.to_string(), value);
            }
        }
    }
    Value::Object(out)
}

/// One dotted key out of a TOML document, as JSON. `null` when it is not there.
fn json_at(doc: &toml::Value, key: &str) -> Value {
    let mut here = doc;
    for part in key.split('.') {
        match here.get(part) {
            Some(next) => here = next,
            None => return Value::Null,
        }
    }
    serde_json::to_value(here).unwrap_or(Value::Null)
}

/// Whether the file actually writes this key down, as opposed to taking the
/// default for it.
fn path_present(doc: &toml::Value, key: &str) -> bool {
    let mut here = doc;
    for part in key.split('.') {
        match here.get(part) {
            Some(next) => here = next,
            None => return false,
        }
    }
    true
}

// --- config.set --------------------------------------------------------------

/// `config.set`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SetConfigRequest {
    /// The keys to change, by their dotted path. Only the keys named are
    /// touched; everything else in the file is left exactly as it is, comments
    /// included.
    #[serde(default)]
    pub changes: BTreeMap<String, Value>,
}

/// What a save did.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SetConfigResult {
    /// Keys the running mixer is on now.
    pub applied: Vec<String>,
    /// Keys written to the file that the mixer picks up the next time it
    /// starts.
    pub needs_restart: Vec<String>,
    /// Keys that were already at the value asked for.
    pub unchanged: Vec<String>,
    /// True when `core.restart` would work here, so a surface knows whether to
    /// offer a button or a sentence.
    pub supervised: bool,
    /// The file that was written.
    pub path: String,
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SetConfigRequest = call.params(&params)?;
    if req.changes.is_empty() {
        return Err(RpcError::invalid_params(
            "config.set was given no changes. Pass `changes` as a map of dotted key to \
             value, for example {\"program.video_bitrate_kbps\": 4500}. `config.get` \
             lists every key you may send.",
        ));
    }
    let path = call.app.config_path.as_path().to_path_buf();
    if path.as_os_str().is_empty() || !path.exists() {
        return Err(RpcError::not_in_state(
            "this core has no config file to write to, so its settings can only be \
             changed by the program that embedded it. Start it with `--config \
             godwinmix.toml` to get one.",
        )
        .with("config_path", path.display().to_string()));
    }

    let typed = typed_changes(&req.changes)?;
    let running = call.app.config();
    let text = std::fs::read_to_string(&path)
        .map_err(|e| RpcError::internal(format!("reading {}: {e}", path.display())))?;

    // One key at a time, so the refusal names the key that caused it rather
    // than handing back a parser error about a file the caller never wrote.
    let mut prefix: Vec<(String, Setting)> = Vec::new();
    for (key, setting) in &typed {
        prefix.push((key.clone(), setting.clone()));
        let candidate = config::with_keys(&text, &prefix, &running)
            .map_err(|e| refused(key, &e))?;
        Config::from_toml(&candidate, "the config file").map_err(|e| refused(key, &e))?;
    }
    let written = config::with_keys(&text, &typed, &running)
        .map_err(|e| RpcError::internal(format!("editing {}: {e:#}", path.display())))?;
    let parsed = Config::from_toml(&written, "the config file")
        .map_err(|e| RpcError::internal(format!("the edited config will not parse: {e:#}")))?;

    let outcome = split_by_timing(&typed, &running, &parsed);
    if call.dry_run {
        return Ok(call.dry_run_answer(
            !outcome.applied.is_empty() || !outcome.needs_restart.is_empty(),
            describe(&typed, &running, &parsed),
        ));
    }

    config::write_atomically(&path, &written)
        .map_err(|e| RpcError::internal(format!("saving {}: {e:#}", path.display())))?;

    // The hot half. `[safety]` is the only table the running mixer can take a
    // change to; `godwinmix_core::config::SETTABLE` says why, key by key.
    if typed.iter().any(|(key, _)| key.starts_with("safety.")) {
        call.app.set_safety(parsed.safety.clone());
    }
    let mut next = (*running).clone();
    next.safety = parsed.safety.clone();
    call.app.set_config(next);

    tracing::info!(
        trace_id = %call.trace_id,
        token = %call.token.id,
        applied = ?outcome.applied,
        needs_restart = ?outcome.needs_restart,
        "settings changed"
    );

    body(SetConfigResult {
        applied: outcome.applied,
        needs_restart: outcome.needs_restart,
        unchanged: outcome.unchanged,
        supervised: supervised(),
        path: path.display().to_string(),
    })
}

/// The incoming JSON, checked against the allow list and given the TOML type
/// its key wants.
fn typed_changes(changes: &BTreeMap<String, Value>) -> Result<Vec<(String, Setting)>, RpcError> {
    let mut out = Vec::with_capacity(changes.len());
    for (key, value) in changes {
        let Some(row) = config::settable(key) else {
            return Err(unknown_key(key));
        };
        let setting = match (row.kind, value) {
            (Kind::Int, Value::Number(n)) if n.is_i64() => Setting::Int(n.as_i64().unwrap_or(0)),
            // A browser's JSON has no integers, so 4500.0 arrives where 4500
            // was typed. A round number is taken; 4500.5 is not.
            (Kind::Int, Value::Number(n)) => match n.as_f64() {
                Some(f) if f.fract() == 0.0 && f.abs() < 9e15 => Setting::Int(f as i64),
                _ => return Err(wrong_kind(row, value)),
            },
            (Kind::Bool, Value::Bool(b)) => Setting::Bool(*b),
            (Kind::Text, Value::String(s)) => Setting::Text(s.clone()),
            _ => return Err(wrong_kind(row, value)),
        };
        out.push((key.clone(), setting));
    }
    // In the allow list's own order, so the file, the log line and the answer
    // all read the same way whatever order the caller sent.
    out.sort_by_key(|(key, _)| {
        config::SETTABLE.iter().position(|r| r.key == *key).unwrap_or(usize::MAX)
    });
    Ok(out)
}

/// What became of each key asked for.
struct Outcome {
    applied: Vec<String>,
    needs_restart: Vec<String>,
    unchanged: Vec<String>,
}

fn split_by_timing(
    typed: &[(String, Setting)],
    running: &Config,
    parsed: &Config,
) -> Outcome {
    let before = toml::Value::try_from(running).unwrap_or_else(|_| toml::Value::Table(toml::Table::new()));
    let after = toml::Value::try_from(parsed).unwrap_or_else(|_| toml::Value::Table(toml::Table::new()));
    let mut outcome = Outcome { applied: Vec::new(), needs_restart: Vec::new(), unchanged: Vec::new() };
    for (key, _) in typed {
        let row = config::settable(key).expect("already checked against the allow list");
        let moved = json_at(&before, key) != json_at(&after, key);
        match (row.timing, moved) {
            (_, false) => outcome.unchanged.push(key.clone()),
            (Timing::Hot, true) => outcome.applied.push(key.clone()),
            (Timing::Restart, true) => outcome.needs_restart.push(key.clone()),
        }
    }
    outcome
}

/// The lines a dry run answers with: one per key, saying where it is going and
/// when it would get there.
fn describe(typed: &[(String, Setting)], running: &Config, parsed: &Config) -> Vec<String> {
    let before = toml::Value::try_from(running).unwrap_or_else(|_| toml::Value::Table(toml::Table::new()));
    let after = toml::Value::try_from(parsed).unwrap_or_else(|_| toml::Value::Table(toml::Table::new()));
    typed
        .iter()
        .map(|(key, _)| {
            let row = config::settable(key).expect("already checked against the allow list");
            let from = json_at(&before, key);
            let to = json_at(&after, key);
            if from == to {
                return format!("{key} is already {to}");
            }
            match row.timing {
                Timing::Hot => format!("{key}: {from} becomes {to}, in force at once"),
                Timing::Restart => {
                    format!("{key}: {from} becomes {to} in the file, in force after a restart")
                }
            }
        })
        .collect()
}

/// A key nobody may set, with the nearest ones that exist.
fn unknown_key(key: &str) -> RpcError {
    let table = key.split('.').next().unwrap_or_default();
    let mut near: Vec<&str> = config::SETTABLE
        .iter()
        .filter(|r| r.key.starts_with(&format!("{table}.")))
        .map(|r| r.key)
        .collect();
    if near.is_empty() {
        near = config::SETTABLE.iter().map(|r| r.key).collect();
    }
    RpcError::invalid_params(format!(
        "`{key}` is not a setting this method may change. Call `config.get` for the whole \
         list. Tokens, the control bind address, sources and outputs are deliberately not \
         on it; sources and outputs have their own methods."
    ))
    .with("key", key.to_string())
    .with("settable", near.join(", "))
}

/// A value of the wrong shape for its key.
fn wrong_kind(row: &config::Settable, value: &Value) -> RpcError {
    RpcError::invalid_params(format!(
        "`{}` wants {} and was given {}. {} The default is {}.",
        row.key,
        row.kind.as_str(),
        shape_of(value),
        row.about,
        row.default
    ))
    .with("key", row.key)
    .with("wants", row.kind.as_str())
}

/// A key the config type itself refused, with the reason it gave.
fn refused(key: &str, why: &anyhow::Error) -> RpcError {
    RpcError::invalid_params(format!(
        "`{key}` was refused and nothing was written: {why:#}"
    ))
    .with("key", key.to_string())
}

fn shape_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "nothing",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "a table",
    }
}

// --- core.restart ------------------------------------------------------------

/// What `core.restart` answers with.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RestartResult {
    /// Always true when the call is accepted: the process is already on its
    /// way out and this answer is written before it goes.
    pub restarting: bool,
    /// The status the process will leave with, which is what the supervisor
    /// watches for.
    pub exit_code: i32,
}

async fn restart(call: Call, _params: Value) -> Result<Value, RpcError> {
    if !supervised() {
        return Err(RpcError::not_in_state(
            "nothing is supervising this mixer, so if it stopped now it would stay \
             stopped and the programme would be off air until somebody started it by \
             hand. Run it from the desktop app, which starts it and starts it again, or \
             from a systemd unit with `Restart=always`, and this will work. Until then, \
             stop it with `core.shutdown` and start it yourself.",
        )
        .with("supervised", false)
        .with(
            "next_step",
            "start the mixer from the desktop app, or add Restart=always and \
             Environment=GODWINMIX_SUPERVISED=1 to its systemd unit",
        ));
    }
    if call.dry_run {
        return Ok(call.dry_run_answer(
            true,
            vec![format!(
                "close the outputs, stop the programme, and exit {} for the supervisor to \
                 start it again",
                crate::EXIT_RESTART
            )],
        ));
    }
    tracing::info!(trace_id = %call.trace_id, token = %call.token.id, "restart requested");
    RESTART_WANTED.store(true, Ordering::SeqCst);
    // The same road out as `core.shutdown`: the session hooks fire, the mixer
    // is told to shut down and its thread is joined, which is what closes the
    // outputs and the recordings properly. Only the status differs.
    call.app.quit.notify_one();
    Ok(json!({ "restarting": true, "exit_code": crate::EXIT_RESTART }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_outside_the_allow_list_is_refused_by_name() {
        let mut changes = BTreeMap::new();
        changes.insert("control.token".to_string(), json!("hunter2"));
        let e = typed_changes(&changes).unwrap_err();
        assert!(e.message.contains("control.token"), "{}", e.message);
        assert!(e.message.contains("config.get"), "{}", e.message);
        assert_eq!(e.data.get("key").and_then(Value::as_str), Some("control.token"));
    }

    #[test]
    fn a_value_of_the_wrong_shape_says_what_the_key_wants() {
        let mut changes = BTreeMap::new();
        changes.insert("program.video_bitrate_kbps".to_string(), json!("lots"));
        let e = typed_changes(&changes).unwrap_err();
        assert!(e.message.contains("wants integer"), "{}", e.message);
        assert!(e.message.contains("was given text"), "{}", e.message);
        assert_eq!(e.data.get("wants").and_then(Value::as_str), Some("integer"));
    }

    #[test]
    fn a_whole_number_arriving_as_a_float_is_taken() {
        // Which is what a browser sends: JSON has one number type and an
        // `<input type="number">` hands back 4500 as 4500.0 often enough.
        let mut changes = BTreeMap::new();
        changes.insert("program.video_bitrate_kbps".to_string(), json!(4500.0));
        changes.insert("multiview.jpeg_quality".to_string(), json!(40.5));
        assert!(typed_changes(&changes).is_err(), "40.5 is not a quality");
        let mut changes = BTreeMap::new();
        changes.insert("program.video_bitrate_kbps".to_string(), json!(4500.0));
        assert_eq!(
            typed_changes(&changes).unwrap(),
            vec![("program.video_bitrate_kbps".to_string(), Setting::Int(4500))]
        );
    }

    #[test]
    fn changes_come_back_in_the_allow_lists_order() {
        let mut changes = BTreeMap::new();
        changes.insert("safety.min_hold_ms".to_string(), json!(500));
        changes.insert("canvas.width".to_string(), json!(1280));
        let typed = typed_changes(&changes).unwrap();
        let keys: Vec<&str> = typed.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["canvas.width", "safety.min_hold_ms"]);
    }

    #[test]
    fn a_change_is_split_into_what_is_on_now_and_what_is_waiting() {
        let running = Config::from_toml("", "running").unwrap();
        let parsed = Config::from_toml(
            "[safety]\nmin_hold_ms = 1200\n[canvas]\nwidth = 1280\nheight = 720\nfps = 30\n\
             sample_rate = 48000\nchannels = 2\n",
            "parsed",
        )
        .unwrap();
        let typed = vec![
            ("canvas.width".to_string(), Setting::Int(1280)),
            ("canvas.fps".to_string(), Setting::Int(30)),
            ("safety.min_hold_ms".to_string(), Setting::Int(1200)),
        ];
        let outcome = split_by_timing(&typed, &running, &parsed);
        assert_eq!(outcome.applied, vec!["safety.min_hold_ms".to_string()]);
        assert_eq!(outcome.needs_restart, vec!["canvas.width".to_string()]);
        // 30 was already the frame rate, so it is not something to restart for.
        assert_eq!(outcome.unchanged, vec!["canvas.fps".to_string()]);
    }

    #[test]
    fn a_dry_run_says_where_each_key_is_going() {
        let running = Config::from_toml("", "running").unwrap();
        let parsed = Config::from_toml("[safety]\nmin_hold_ms = 1200\n", "parsed").unwrap();
        let typed = vec![("safety.min_hold_ms".to_string(), Setting::Int(1200))];
        let lines = describe(&typed, &running, &parsed);
        assert_eq!(lines, vec!["safety.min_hold_ms: 0 becomes 1200, in force at once".to_string()]);
    }

    #[test]
    fn nothing_secret_survives_the_answer() {
        let cfg = Config::from_toml(
            "[control]\nbind = \"0.0.0.0:8080\"\ntoken = \"hunter2\"\n\
             [[outputs]]\nid = \"yt\"\ntype = \"rtmp\"\n\
             uri = \"rtmp://a.rtmp.youtube.com/live2/abcd-efgh-ijkl\"\n",
            "secrets",
        )
        .unwrap();
        let clean = redacted(&cfg).unwrap();
        let text = toml::to_string(&clean).unwrap();
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains("abcd-efgh-ijkl"), "{text}");
        // And the answer a caller actually gets carries neither table at all.
        let answer = settable_tables(&clean);
        assert!(answer.get("control").is_none(), "{answer}");
        assert!(answer.get("outputs").is_none(), "{answer}");
        assert!(answer.get("safety").is_some(), "{answer}");
    }

    #[test]
    fn supervision_is_something_the_core_is_told_and_never_guesses() {
        // One test rather than two, because the environment belongs to the
        // whole process and two of these running at once would each see the
        // other's variable.
        std::env::remove_var("GODWINMIX_SUPERVISED");
        std::env::remove_var("LIVEBOXMIX_SUPERVISED");
        assert!(!supervised(), "a core told nothing is a core nobody is watching");
        for (value, expected) in
            [("1", true), ("true", true), ("yes", true), ("0", false), ("false", false), ("", false)]
        {
            std::env::set_var("GODWINMIX_SUPERVISED", value);
            assert_eq!(supervised(), expected, "GODWINMIX_SUPERVISED={value}");
        }
        std::env::remove_var("GODWINMIX_SUPERVISED");

        // And the refusal an unsupervised core gives names the state and the
        // next step, in the message and in the data beside it.
        let refusal = RpcError::not_in_state("nothing is supervising this mixer")
            .with("supervised", false)
            .with("next_step", "start the mixer from the desktop app");
        assert_eq!(refusal.data.get("supervised").and_then(Value::as_bool), Some(false));
        assert!(refusal.data.get("next_step").is_some());
    }
}
