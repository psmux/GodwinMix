//! `core.api`, `godwinmix --api-info`, `protocol.json` and `protocol.md`.
//!
//! All four are the same document. The method table, the event table and the
//! types with `schemars` derives are walked once and written out; the markdown
//! is rendered from that JSON rather than kept beside it, so the human
//! reference cannot describe a method the core does not have.
//!
//! `protocol.json` is committed. A test regenerates it and fails on any
//! difference, which is the CI drift check.

use crate::error::ErrorCode;
use crate::method::{schema_of, Registry, SchemaFn};
use crate::requests;
use crate::types;
use crate::{API_COMPATIBLE, API_LEVEL};
use schemars::{generate::SchemaSettings, SchemaGenerator};
use serde_json::{json, Map, Value};

/// One event the core publishes.
pub struct EventDef {
    /// Without the `event/` prefix. `core.subscribe` patterns match this.
    pub name: &'static str,
    pub since: &'static str,
    pub summary: &'static str,
    /// The `ext` key that has to be asked for before this event is sent.
    pub ext: Option<&'static str>,
    /// The name this carried on the legacy `/ws` stream, where it had one.
    pub legacy: Option<&'static str>,
    pub payload: SchemaFn,
}

fn inline(value: Value) -> Value {
    value
}

/// The event table from 03 section 6, as far as this build implements it.
///
/// Events the plan names but that nothing raises yet (`scene.*`,
/// `plugin.state`) are deliberately absent: publishing a schema for something
/// that never arrives teaches a client to wait for it.
pub fn events() -> Vec<EventDef> {
    let mut all = state_events();
    all.extend(stream_events());
    all
}

/// The events every subscriber gets: what changed, and the bookkeeping that
/// lets a client know it has the whole story.
fn state_events() -> Vec<EventDef> {
    vec![
        EventDef {
            name: "snapshot",
            since: "1",
            summary: "The full state, and the sequence number it is current as of. \
                      Sent on subscribe and after any change the deltas cannot describe.",
            ext: None,
            legacy: Some("status"),
            payload: schema_of::<requests::Snapshot>,
        },
        EventDef {
            name: "program.took",
            since: "1",
            summary: "The programme changed. Carries the running time the cut landed on, \
                      so a client can see how close a scheduled take was to its mark.",
            ext: None,
            legacy: Some("took"),
            payload: |_| {
                inline(json!({
                    "type": "object",
                    "properties": {
                        "source": { "type": ["string", "null"] },
                        "scene": { "type": ["string", "null"] },
                        "transition": { "type": "string" },
                        "duration_ms": { "type": "integer" },
                        "at_running_time_ms": { "type": "integer" }
                    }
                }))
            },
        },
        EventDef {
            name: "preview.changed",
            since: "1",
            summary: "A scene was armed, or the arming was cleared. The armed scene is the \
                      preview, and program.take with no argument takes it.",
            ext: None,
            legacy: None,
            payload: |_| {
                inline(json!({
                    "type": "object",
                    "properties": { "scene": { "type": ["string", "null"] } }
                }))
            },
        },
        EventDef {
            name: "source.state",
            since: "1",
            summary: "A source moved between connecting, live, stalled and failed.",
            ext: None,
            legacy: Some("source_state_changed"),
            payload: |g| {
                json!({
                    "type": "object",
                    "properties": {
                        "source": { "type": "string" },
                        "state": schema_of::<types::SourceState>(g),
                        "detail": { "type": ["string", "null"] }
                    }
                })
            },
        },
        EventDef {
            name: "source.position",
            since: "1",
            summary: "How far through a seekable source has got, a few times a second. \
                      Never sent for a camera, which has no position to report.",
            ext: Some("positions"),
            legacy: Some("source_position"),
            payload: |_| {
                inline(json!({
                    "type": "object",
                    "properties": {
                        "source": { "type": "string" },
                        "position_ms": { "type": "integer" },
                        "duration_ms": { "type": ["integer", "null"] }
                    }
                }))
            },
        },
        EventDef {
            name: "output.state",
            since: "1",
            summary: "A destination connected, dropped or is retrying.",
            ext: None,
            legacy: Some("output_state_changed"),
            payload: |g| {
                json!({
                    "type": "object",
                    "properties": {
                        "output": { "type": "string" },
                        "state": schema_of::<types::OutputState>(g),
                        "reconnects": { "type": "integer" }
                    }
                })
            },
        },
        EventDef {
            name: "adbreak.changed",
            since: "1",
            summary: "An ad break was armed, went on air, or ended.",
            ext: None,
            legacy: Some("ad_break_changed"),
            payload: |g| {
                json!({
                    "type": "object",
                    "properties": { "ad": schema_of::<Option<types::AdStatus>>(g) }
                })
            },
        },
        EventDef {
            name: "ui.changed",
            since: "1",
            summary: "The surface defaults changed: a preset was applied, or an operator set \
                      the layout, theme or gallery mode by hand. Nothing on air moves.",
            ext: None,
            legacy: None,
            payload: |g| {
                json!({
                    "type": "object",
                    "properties": { "ui": schema_of::<types::UiDefaults>(g) }
                })
            },
        },
        EventDef {
            name: "hook.blocked",
            since: "1",
            summary: "A hook did not get its say: it did not answer inside its timeout, or \
                      the thing behind it could not be reached. Whatever the hook was \
                      attached to went ahead anyway, which is the rule that keeps a slow \
                      hook off the frame path. See 03 section 8.",
            ext: None,
            legacy: None,
            payload: |_| {
                inline(json!({
                    "type": "object",
                    "properties": {
                        "hook": { "type": "string", "description": "The hook name, for example take.before." },
                        "plugin": { "type": "string", "description": "The plugin that owns it, or the URL or command when it came from [[hooks]] in the config." },
                        "reason": { "type": "string", "description": "What went wrong and what to do about it." }
                    },
                    "required": ["hook", "plugin", "reason"]
                }))
            },
        },
        EventDef {
            name: "media.changed",
            since: "1",
            summary: "A file in the library was uploaded, deleted, or its conversion moved on.",
            ext: None,
            legacy: Some("media_changed"),
            payload: |_| {
                inline(json!({
                    "type": "object",
                    "properties": { "name": { "type": "string" }, "conversion": {} }
                }))
            },
        },
    ]
}

/// The events behind an `ext` key, plus the resync and flush markers that end
/// every batch.
fn stream_events() -> Vec<EventDef> {
    vec![
        EventDef {
            name: "meters",
            since: "1",
            summary: "Peak dBFS for the programme bus and every source, in one message at \
                      10 per second. Replaces the two separate meter events on /ws.",
            ext: Some("meters"),
            legacy: Some("audio_level, source_audio_level"),
            payload: schema_of::<requests::Meters>,
        },
        EventDef {
            name: "tally",
            since: "1",
            summary: "Which sources are on programme, on preview, or off. Derived by the \
                      core so a Stream Deck does not have to.",
            ext: Some("tally"),
            legacy: None,
            payload: schema_of::<requests::Tally>,
        },
        EventDef {
            name: "alert",
            since: "1",
            summary: "Something an operator should see. Also written to the log and to the \
                      alert webhook.",
            ext: None,
            legacy: Some("alert"),
            payload: |g| {
                json!({
                    "type": "object",
                    "properties": {
                        "severity": schema_of::<types::Severity>(g),
                        "message": { "type": "string" }
                    }
                })
            },
        },
        EventDef {
            name: "telemetry",
            since: "1",
            summary: "Numbers instead of a picture, up to ten times a second and under 200 \
                      bytes: the shot change score, the black ratio, a freeze flag, short \
                      term and integrated loudness, a silence flag and which sources are \
                      live. From cheap probes on the raw programme frames, which run only \
                      while a client is subscribed.",
            ext: Some("telemetry"),
            legacy: None,
            payload: |_| {
                inline(json!({
                    "type": "object",
                    "properties": {
                        "ts": { "type": "integer", "description": "milliseconds since the Unix epoch" },
                        "shot": { "type": "number", "description": "how much the picture changed since the last frame, 0 to 1" },
                        "black": { "type": "number", "description": "fraction of the picture at or below black, 0 to 1" },
                        "freeze": { "type": "boolean" },
                        "lufs_s": { "type": ["number", "null"], "description": "short term loudness over three seconds, approximated from the programme meter" },
                        "lufs_i": { "type": ["number", "null"] },
                        "silence": { "type": "boolean" },
                        "sources": {
                            "type": "object",
                            "additionalProperties": { "type": "integer" },
                            "description": "source id to 1 when it is live and 0 otherwise"
                        }
                    },
                    "required": ["ts", "shot", "black", "freeze", "silence", "sources"]
                }))
            },
        },
        EventDef {
            name: "agent.state",
            since: "1",
            summary: "The agent.state document, pushed when a telemetry threshold crosses or \
                      a take lands, with `why` naming which and a snapshot URL beside it. \
                      Edge triggered and at most one a second, so a picture that stays black \
                      is one message rather than one a tick.",
            ext: Some("agent"),
            legacy: None,
            payload: |_| {
                inline(json!({
                    "type": "object",
                    "description": "The concise agent.state document, plus `why` (one of \
                                    program, black, freeze, silence, shot) and `snapshot`.",
                    "additionalProperties": true
                }))
            },
        },
        EventDef {
            name: "multiview.layout",
            since: "1",
            summary: "How to read the binary frames that follow: the cells, and the layout \
                      id carried in every frame header.",
            ext: Some("multiview"),
            legacy: None,
            payload: schema_of::<requests::MultiviewLayout>,
        },
        EventDef {
            name: "multiview.frame",
            since: "1",
            summary: "A mosaic frame, as a binary WebSocket frame rather than JSON: a 16 \
                      byte little endian header (seq u32, layout id u32, programme running \
                      time in milliseconds u64) then the JPEG.",
            ext: Some("multiview"),
            legacy: Some("raw JPEG binary frame"),
            payload: |_| {
                inline(json!({
                    "type": "string",
                    "contentEncoding": "binary",
                    "description": "16 byte header then JPEG. Not a JSON message."
                }))
            },
        },
        EventDef {
            name: "resync",
            since: "1",
            summary: "This client fell behind and events were dropped. Re-subscribe for a \
                      fresh snapshot; nothing between from_seq and the new snapshot arrives.",
            ext: None,
            legacy: None,
            payload: schema_of::<requests::Resync>,
        },
        EventDef {
            name: "flush",
            since: "1",
            summary: "The end of a batch. Render here and not before, so a client never \
                      paints half an update.",
            ext: None,
            legacy: None,
            payload: schema_of::<requests::Flush>,
        },
    ]
}

/// The authoritative `ext` table, for the reference and for a client deciding
/// what to ask for.
pub fn ext_table() -> Vec<(&'static str, &'static str, &'static str, bool)> {
    vec![
        (
            "multiview",
            "{fps: 1..30, width: 320..1920} or false",
            "the mosaic pipeline, built on the first subscriber and stopped on the last, \
             plus event/multiview.layout and the binary frames",
            true,
        ),
        ("meters", "true", "event/meters", true),
        ("tally", "true", "event/tally", true),
        ("positions", "true", "event/source.position", true),
        ("thumb", "{fps}", "per source thumbnails from a node", false),
        ("preview", "{fps, width} or \"full\"", "the preview scene", false),
        ("telemetry", "{hz: 1..10}", "event/telemetry", false),
        ("agent", "true or thresholds", "event/agent.state with a snapshot URL", false),
    ]
}

/// The routes that existed before `/api/v1`, and the method each is now an
/// alias for.
///
/// Published rather than kept quietly in control.rs, because a client on the
/// old paths needs to be able to look up what to move to, and because the
/// route coverage test reads this list to prove that nothing in the router is
/// undocumented.
pub const LEGACY_ROUTES: &[(&str, &str, &str)] = &[
    ("GET", "/api/status", "core.status"),
    ("GET", "/api/agent/state", "agent.state"),
    ("GET", "/api/snapshot/{name}", "snapshot.get"),
    ("POST", "/api/take", "program.take"),
    ("POST", "/api/golive", "program.golive"),
    ("POST", "/api/shutdown", "core.shutdown"),
    ("GET", "/api/media", "media.list"),
    ("POST", "/api/media/upload", "media.upload"),
    ("POST", "/api/media/{name}/convert", "media.convert"),
    ("DELETE", "/api/media/{name}", "media.remove"),
    ("POST", "/api/adbreak", "adbreak.start"),
    ("POST", "/api/adbreak/end", "adbreak.end"),
    ("POST", "/api/sources", "source.add"),
    ("DELETE", "/api/sources/{id}", "source.remove"),
    ("POST", "/api/sources/{id}/audio", "source.audio.set"),
    ("POST", "/api/sources/{id}/seek", "source.seek"),
    ("GET", "/api/outputs", "output.list"),
    ("POST", "/api/outputs", "output.add"),
    ("DELETE", "/api/outputs/{id}", "output.remove"),
    ("POST", "/api/outputs/{id}/reconnect", "output.reconnect"),
    ("GET", "/ws", "core.subscribe"),
];

/// Paths that are not methods: the page, and the WebSocket the protocol runs
/// over.
pub const WELL_KNOWN: &[(&str, &str, &str)] = &[
    ("GET", "/", "the reference web UI, served from the binary"),
    ("GET", "/rpc", "the JSON-RPC WebSocket. Everything in `methods` is reachable here."),
    ("GET", "/api/v1/status", "an alias for GET /api/v1/core/status, because it is what people type"),
    ("ANY", "/api/v1/{*rest}", "every method's REST route, generated by the transform rule"),
];

/// Build the whole document.
pub fn descriptor<C>(registry: &Registry<C>, kinds: Value) -> Value {
    let mut g = SchemaSettings::draft2020_12().into_generator();

    let methods: Vec<Value> = registry.iter().map(|m| method_entry(m, &mut g)).collect();
    let events: Vec<Value> = events().iter().map(|e| event_entry(e, &mut g)).collect();
    let defs = g.take_definitions(true);

    json!({
        "core": "godwinmix",
        "api_level": API_LEVEL,
        "api_compatible": API_COMPATIBLE,
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "description": "The GodwinMix control protocol. One set of methods, events and \
                        types, whether the peer is a UI on /rpc, curl on /api/v1, the CLI, \
                        the MCP server or a plugin on stdio.",
        "transports": transports(),
        "envelope": envelope(),
        "methods": methods,
        "events": events,
        "legacy": LEGACY_ROUTES
            .iter()
            .map(|(http, path, method)| json!({ "method": http, "path": path, "now": method }))
            .collect::<Vec<_>>(),
        "well_known": WELL_KNOWN
            .iter()
            .map(|(http, path, note)| json!({ "method": http, "path": path, "note": note }))
            .collect::<Vec<_>>(),
        "ext": ext_entries(),
        "kinds": kinds,
        "errors": error_entries(),
        "$defs": defs,
    })
}

fn method_entry<C>(m: &crate::method::MethodDef<C>, g: &mut SchemaGenerator) -> Value {
    let mut entry = Map::new();
    entry.insert("name".into(), json!(m.name));
    entry.insert("since".into(), json!(m.since));
    entry.insert("summary".into(), json!(m.summary));
    entry.insert("scope".into(), json!(m.scope.as_str()));
    entry.insert("destructive".into(), json!(m.destructive));
    entry.insert("mutating".into(), json!(m.mutating));
    entry.insert("idempotent".into(), json!(m.idempotent));
    entry.insert("params".into(), (m.params)(g));
    entry.insert("result".into(), (m.result)(g));
    if let Some(rest) = &m.rest {
        entry.insert("rest".into(), json!({ "method": rest.http, "path": rest.path }));
    }
    if let Some(mcp) = &m.mcp {
        entry.insert(
            "mcp".into(),
            json!({
                "tool": mcp.tool,
                "profile": match mcp.tier {
                    crate::method::Tier::Minimal => "minimal",
                    crate::method::Tier::Standard => "standard",
                    crate::method::Tier::Search => "search",
                },
                "readOnlyHint": !m.mutating,
                "destructiveHint": m.destructive,
                "idempotentHint": m.idempotent,
            }),
        );
    }
    Value::Object(entry)
}

fn event_entry(e: &EventDef, g: &mut SchemaGenerator) -> Value {
    json!({
        "name": format!("event/{}", e.name),
        "pattern": e.name,
        "since": e.since,
        "summary": e.summary,
        "ext": e.ext,
        "legacy": e.legacy,
        "payload": (e.payload)(g),
    })
}

fn ext_entries() -> Vec<Value> {
    ext_table()
        .into_iter()
        .map(|(key, value, turns_on, implemented)| {
            json!({ "key": key, "value": value, "turns_on": turns_on, "implemented": implemented })
        })
        .collect()
}

fn error_entries() -> Vec<Value> {
    ErrorCode::ALL
        .iter()
        .map(|c| {
            json!({
                "code": c.number(),
                "meaning": c.meaning(),
                "retryable": c.retryable(),
                "http_status": c.http_status(),
            })
        })
        .collect()
}

fn transports() -> Value {
    json!([
        { "peer": "UI, service, node", "at": "WebSocket /rpc",
          "framing": "one JSON-RPC message per text frame; mosaic frames are binary" },
        { "peer": "curl, <img>", "at": "/api/v1",
          "framing": "the REST transform of the method names, one error shape" },
        { "peer": "legacy UI", "at": "/api and /ws",
          "framing": "deprecated aliases, kept for one release, answered with a Deprecation header" },
        { "peer": "MCP client", "at": "stdio",
          "framing": "godwinmix mcp, a thin adapter over these same methods" }
    ])
}

fn envelope() -> Value {
    json!({
        "description": "Keys accepted on every method, handled before a method runs.",
        "properties": {
            "trace_id": {
                "type": "string",
                "description": "Carried into the answer, the X-Trace-Id header and the log \
                                line. Taken from the W3C traceparent header over HTTP, or \
                                generated."
            },
            "idempotency_key": {
                "type": "string", "maxLength": 255,
                "description": "On any mutating method. The answer is kept for 24 hours; a \
                                replay returns it with replayed: true. The same key with \
                                different params is -32602 with data.idempotency = mismatch."
            },
            "dry_run": {
                "type": "boolean",
                "description": "On any destructive method. Answers the diff it would make \
                                and would_change, against the live state, and changes nothing."
            },
            "confirm": {
                "type": "string",
                "description": "The confirm_token from a -32020 refusal, valid 30 seconds. \
                                Only a token whose policy is confirm = required needs it."
            }
        }
    })
}

/// The human reference, obs-websocket style, rendered from the same document.
pub fn markdown(doc: &Value) -> String {
    let mut out = String::new();
    out.push_str("# GodwinMix control protocol\n\n");
    out.push_str(&format!(
        "Generated from the method table in `crates/godwinmix-protocol/`. Do not edit by hand: \
         `cargo test protocol_json_is_current` fails when this file and the code disagree, \
         and `godwinmix --api-info` prints the JSON behind it.\n\n\
         * `api_level`: {}\n* `api_compatible`: {}\n\n",
        doc["api_level"], doc["api_compatible"]
    ));
    out.push_str(doc["description"].as_str().unwrap_or_default());
    out.push_str("\n\n");
    markdown_transports(doc, &mut out);
    markdown_envelope(doc, &mut out);
    markdown_methods(doc, &mut out);
    markdown_events(doc, &mut out);
    markdown_legacy(doc, &mut out);
    markdown_ext(doc, &mut out);
    markdown_errors(doc, &mut out);
    out
}

fn markdown_transports(doc: &Value, out: &mut String) {
    out.push_str("## Transports\n\n| Peer | Where | Framing |\n|---|---|---|\n");
    for t in doc["transports"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "| {} | `{}` | {} |\n",
            text(&t["peer"]),
            text(&t["at"]),
            text(&t["framing"])
        ));
    }
    out.push('\n');
}

fn markdown_envelope(doc: &Value, out: &mut String) {
    out.push_str("## On every call\n\n");
    out.push_str(text(&doc["envelope"]["description"]));
    out.push_str("\n\n| Key | Type | What it does |\n|---|---|---|\n");
    for (key, spec) in doc["envelope"]["properties"].as_object().into_iter().flatten() {
        out.push_str(&format!(
            "| `{key}` | {} | {} |\n",
            text(&spec["type"]),
            text(&spec["description"])
        ));
    }
    out.push('\n');
}

fn markdown_methods(doc: &Value, out: &mut String) {
    out.push_str("## Methods\n\n");
    out.push_str(
        "`scope` is the least a token needs. A destructive method accepts `dry_run` and, on a \
         token whose policy is `confirm = required`, needs a confirm token first.\n\n",
    );
    out.push_str("| Method | REST | Scope | Destructive | Since | What it does |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for m in doc["methods"].as_array().into_iter().flatten() {
        let rest = match m.get("rest") {
            Some(r) => format!("`{} {}`", text(&r["method"]), text(&r["path"])),
            None => "(none)".to_string(),
        };
        out.push_str(&format!(
            "| `{}` | {rest} | {} | {} | {} | {} |\n",
            text(&m["name"]),
            text(&m["scope"]),
            if m["destructive"] == json!(true) { "yes" } else { "" },
            text(&m["since"]),
            text(&m["summary"]),
        ));
    }
    out.push('\n');
    out.push_str("### Params and results\n\n");
    for m in doc["methods"].as_array().into_iter().flatten() {
        out.push_str(&format!("#### `{}`\n\n", text(&m["name"])));
        out.push_str(&format!("{}\n\n", text(&m["summary"])));
        if let Some(mcp) = m.get("mcp") {
            out.push_str(&format!(
                "MCP tool `{}` in the `{}` profile: readOnlyHint {}, destructiveHint {}, \
                 idempotentHint {}.\n\n",
                text(&mcp["tool"]),
                text(&mcp["profile"]),
                mcp["readOnlyHint"],
                mcp["destructiveHint"],
                mcp["idempotentHint"],
            ));
        }
        out.push_str("```json\n");
        out.push_str(&format!(
            "{{\n  \"params\": {},\n  \"result\": {}\n}}\n",
            pretty(&m["params"]),
            pretty(&m["result"])
        ));
        out.push_str("```\n\n");
    }
}

fn markdown_events(doc: &Value, out: &mut String) {
    out.push_str("## Events\n\n");
    out.push_str(
        "Subscribe with `core.subscribe`. Patterns match the part after `event/`, so \
         `program.*` matches `event/program.took`. Every event carries `seq`; every batch \
         ends with `event/flush`; a client that falls behind gets `event/resync`.\n\n",
    );
    out.push_str("| Event | ext | Replaces on /ws | What it carries |\n|---|---|---|---|\n");
    for e in doc["events"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            text(&e["name"]),
            e["ext"].as_str().map(|s| format!("`{s}`")).unwrap_or_default(),
            e["legacy"].as_str().map(|s| format!("`{s}`")).unwrap_or_default(),
            text(&e["summary"]),
        ));
    }
    out.push('\n');
}

fn markdown_legacy(doc: &Value, out: &mut String) {
    out.push_str("## The routes this replaces\n\n");
    out.push_str(
        "The paths below still answer, for one release, with a `Deprecation: true` header. \
         Move to the method named beside each one.\n\n",
    );
    out.push_str("| Was | Now |\n|---|---|\n");
    for r in doc["legacy"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "| `{} {}` | `{}` |\n",
            text(&r["method"]),
            text(&r["path"]),
            text(&r["now"])
        ));
    }
    out.push_str("\n| Path | What it is |\n|---|---|\n");
    for r in doc["well_known"].as_array().into_iter().flatten() {
        out.push_str(&format!("| `{} {}` | {} |\n", text(&r["method"]), text(&r["path"]), text(&r["note"])));
    }
    out.push('\n');
}

fn markdown_ext(doc: &Value, out: &mut String) {
    out.push_str("## The ext table\n\n");
    out.push_str(
        "A client declares which expensive streams it wants. The core does no work for a \
         stream nobody asked for. An `ext` key is the subscription for its own events: ask \
         for `meters` and you get `event/meters`, whether or not `meters` is among your \
         event patterns. Keys marked not implemented are accepted and reported back in \
         `ignored_ext`, so a client written against the whole table still connects.\n\n",
    );
    out.push_str("| Key | Value | Turns on | In this build |\n|---|---|---|---|\n");
    for e in doc["ext"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} |\n",
            text(&e["key"]),
            text(&e["value"]),
            text(&e["turns_on"]),
            if e["implemented"] == json!(true) { "yes" } else { "not yet" },
        ));
    }
    out.push('\n');
}

fn markdown_errors(doc: &Value, out: &mut String) {
    out.push_str("## Errors\n\n");
    out.push_str(
        "One shape everywhere: `{\"error\": {\"code\", \"message\", \"data\"}}`, with \
         `trace_id` beside it. The message names the current state and the next step, and \
         an unknown id lists the ids that would have worked.\n\n",
    );
    out.push_str("| Code | Meaning | HTTP | Retryable |\n|---|---|---|---|\n");
    for e in doc["errors"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            e["code"],
            text(&e["meaning"]),
            e["http_status"],
            if e["retryable"] == json!(true) { "yes" } else { "no" },
        ));
    }
    out.push('\n');
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".into()).replace('\n', "\n  ")
}

/// The bytes written to `protocol.json`, ending in a newline so that the file
/// is a well behaved text file and git does not complain.
pub fn json_text(doc: &Value) -> String {
    let mut text = serde_json::to_string_pretty(doc).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_in_the_table_has_a_name_a_summary_and_a_payload() {
        let mut g = SchemaSettings::draft2020_12().into_generator();
        for e in events() {
            assert!(!e.name.is_empty());
            assert!(e.summary.len() > 20, "{} needs a real summary", e.name);
            let payload = (e.payload)(&mut g);
            assert!(payload.is_object(), "{} has no payload schema", e.name);
        }
    }

    /// The ext keys the code acts on and the ext keys the table publishes as
    /// implemented have to be the same list, or a client is told to ask for a
    /// stream that never starts.
    #[test]
    fn the_published_ext_table_matches_what_the_subscription_struct_reads() {
        let implemented: Vec<&str> =
            ext_table().into_iter().filter(|(_, _, _, yes)| *yes).map(|(k, _, _, _)| k).collect();
        assert_eq!(implemented, vec!["multiview", "meters", "tally", "positions"]);
        // And every implemented key is one `Ext` has a field for.
        let ext: requests::Ext = serde_json::from_value(json!({
            "multiview": false, "meters": true, "tally": true, "positions": true
        }))
        .unwrap();
        assert!(ext.unsupported().is_empty(), "a published key landed in `other`");
    }
}
