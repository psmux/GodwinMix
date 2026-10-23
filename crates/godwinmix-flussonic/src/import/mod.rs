//! A Flussonic config as direct shows: each `stream` one show, its input
//! the show's input, and one HLS output that carries the transcoder's
//! rendition, or a copy when there is no transcoder.
//!
//! Nothing is silently ignored. Every directive the typed view does not know,
//! every transcoder option without a home and every field still waiting on
//! another branch is a [`Note`] with its line, in [`Import::notes`].

mod codecs;
mod input;
mod want;

pub use input::redact;
pub use want::WAITING;

use crate::model::{Config, Stream};
use serde::Serialize;
use serde_json::{json, Map, Value};

/// The id of the one HLS output every imported show has.
pub const HLS_OUTPUT: &str = "flussonic";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// Read and not acted on: a directive or option GodwinMix has no use for.
    Ignored,
    /// Understood, but GodwinMix does something else, said in the note.
    NotCarried,
    /// Written into the request under the name the media branch plans; the
    /// station does not read it yet.
    Waiting,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    pub line: usize,
    pub kind: NoteKind,
    pub what: String,
}

impl Note {
    pub fn ignored(line: usize, what: String) -> Note {
        Note { line, kind: NoteKind::Ignored, what }
    }

    pub fn not_carried(line: usize, what: String) -> Note {
        Note { line, kind: NoteKind::NotCarried, what }
    }
}

/// One stream as a direct show. `input` and `outputs` are what
/// `show.add_many` takes, as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct ShowPlan {
    /// Flussonic's name, which is the public id in every URL.
    pub name: String,
    pub title: Option<String>,
    pub disabled: bool,
    pub input: Value,
    pub outputs: Vec<Value>,
    /// No transcoder: the input is served as it comes.
    pub copy: bool,
}

impl ShowPlan {
    /// Whether running this plan means restarting a show that runs `other`.
    pub fn same_media(&self, other: &ShowPlan) -> bool {
        self.input == other.input && self.outputs == other.outputs
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Import {
    pub shows: Vec<ShowPlan>,
    pub notes: Vec<(String, Note)>,
}

/// The HLS output's params from `segment_duration` and `segment_count`.
/// Every stream's HLS output makes MPEG-TS beside CMAF, because Flussonic's
/// paths serve TS segments, and answers without a viewer key, because
/// Flussonic served playback with none and whatever sits in front of it does
/// its own checking. `open` still answers only loopback and private addresses.
fn hls_params(cfg: &Config, s: &Stream) -> Option<Value> {
    let duration = s.segment_duration.or(cfg.segment_duration);
    let count = s.segment_count.or(cfg.segment_count);
    let mut p = Map::new();
    p.insert("segments".into(), json!("both"));
    p.insert("access".into(), json!("open"));
    if let Some(d) = duration {
        p.insert("segment_ms".into(), json!(((d * 1000.0).round() as u32).clamp(500, 10_000)));
    }
    if let Some(c) = count {
        let window = (f64::from(c) * duration.unwrap_or(2.0)).ceil() as u32;
        p.insert("window".into(), json!(window.max(1)));
    }
    Some(Value::Object(p))
}

fn plan(cfg: &Config, s: &Stream, notes: &mut Vec<Note>) -> Option<ShowPlan> {
    let Some(first) = s.inputs.first() else {
        notes.push(Note::not_carried(s.line, "a stream with no input: nothing to run".into()));
        return None;
    };
    let mut input = input::spec(first, s.line, notes);
    if let Some(second) = s.inputs.get(1) {
        input["backup"] = input::spec(second, s.line, notes);
    }
    for extra in s.inputs.iter().skip(2) {
        notes.push(Note::not_carried(s.line, format!("a third input ({}): a show has a main and one backup", redact(&extra.url))));
    }
    let rendition = s.transcoder.as_ref().and_then(|t| want::rendition(t, s.line, notes));
    let mut hls = json!({ "id": HLS_OUTPUT, "uri": format!("hls://{HLS_OUTPUT}") });
    if let Some(p) = hls_params(cfg, s) {
        hls["params"] = p;
    }
    if let Some(r) = &rendition {
        hls["rendition"] = r.clone();
    }
    let mut outputs = vec![hls];
    for (i, push) in s.pushes.iter().enumerate() {
        let mut out = json!({ "id": format!("push-{}", i + 1), "uri": push });
        if let Some(r) = &rendition {
            out["rendition"] = r.clone();
        }
        outputs.push(out);
    }
    if s.dvr.is_some() {
        notes.push(Note::not_carried(s.line, "dvr: GodwinMix keeps the live window only".into()));
    }
    let copy = rendition.is_none();
    Some(ShowPlan { name: s.name.clone(), title: s.title.clone(), disabled: s.disabled, input, outputs, copy })
}

/// The whole config as shows, with every note.
pub fn import(cfg: &Config) -> Import {
    let mut out = Import::default();
    for u in &cfg.unknown {
        out.notes.push((u.scope.clone(), Note::ignored(u.line, format!("directive '{}'", u.name))));
    }
    for d in &cfg.dvrs {
        out.notes.push(("global".into(), Note::not_carried(0, format!("dvr {}: GodwinMix keeps the live window only", d.name))));
    }
    for s in &cfg.streams {
        let mut notes = Vec::new();
        if let Some(p) = plan(cfg, s, &mut notes) {
            waiting(&p, s.line, &mut notes);
            out.shows.push(p);
        }
        out.notes.extend(notes.into_iter().map(|n| (format!("stream {}", s.name), n)));
    }
    out
}

/// One note per stream naming the fields the station does not read yet.
fn waiting(p: &ShowPlan, line: usize, notes: &mut Vec<Note>) {
    let r = &p.outputs[0]["rendition"];
    let mut held: Vec<&str> = Vec::new();
    for part in ["video", "audio"] {
        if let Some(m) = r[part].as_object() {
            held.extend(m.keys().map(String::as_str).filter(|k| WAITING.contains(k)));
        }
    }
    if !held.is_empty() {
        let what = format!("rendition fields {} are passed on, and this build does not act on them yet", held.join(", "));
        notes.push(Note { line, kind: NoteKind::Waiting, what });
    }
}

#[cfg(test)]
mod tests;
