//! What a running replay source answers: `load` another stretch, `set` the
//! angle or the speed from the frame it is on, and `state`.
//!
//! Neither `load` nor `set` moves the pipeline. Each swaps what the feed
//! plays, holds it, and answers with the position to seek to; the caller
//! seeks through the mixer (`source.seek`), which resets the aligner first,
//! and that seek is what lets the feed go on. So a replay on air changes
//! angle with one flushing seek, the same one a scrubber makes.

use super::replay::{Playing, ReplaySource, MANIFEST};
use crate::plugin::source::unknown_method;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};

const METHODS: &[&str] = &["load", "set", "state", "restart"];

/// The keys a `load` replaces, all of them, so one left out goes back to its
/// default rather than keeping the last load's.
const LOAD_KEYS: [&str; 4] = ["event", "seconds", "angle", "speed"];

impl ReplaySource {
    pub(super) fn replay_call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            "restart" => Ok(Value::Null),
            "state" => Ok(self.state()),
            "load" => self.load(&params),
            "set" => self.set(&params),
            other => Err(unknown_method(&MANIFEST, other, METHODS)),
        }
    }

    fn playing(&self) -> Result<&Playing> {
        self.playing.as_ref().context("this replay source is not playing yet; add it again once replay holds pictures")
    }

    fn state(&self) -> Value {
        let Some(p) = &self.playing else { return json!({ "playing": false }) };
        json!({
            "playing": true,
            "event": p.window.event,
            "angle": p.angle,
            "speed": p.feed.speed(),
            "duration_ms": p.feed.duration() / 1_000_000,
            "position_ms": p.feed.position() / 1_000_000,
        })
    }

    fn load(&mut self, params: &Value) -> Result<Value> {
        let mut next = self.params.clone();
        for key in LOAD_KEYS {
            next.remove(key);
        }
        for (k, v) in params.as_object().into_iter().flatten().filter(|(k, _)| LOAD_KEYS.contains(&k.as_str())) {
            if let Some(v) = toml_of(v) {
                next.insert(k.clone(), v);
            }
        }
        let (window, clip, speed) = self.resolve(&next)?;
        let p = self.playing.as_mut().context("this replay source is not playing yet; add it again")?;
        p.feed.replace(clip, speed);
        p.angle = window.angle.clone();
        p.window = window;
        self.params = next;
        self.retime();
        Ok(json!({ "position_ms": 0, "state": self.state() }))
    }

    fn set(&mut self, params: &Value) -> Result<Value> {
        let p = self.playing()?;
        let shared = self.shared()?;
        let speed = crate::replay::shared::check_speed(params.get("speed").and_then(Value::as_f64).or(Some(p.feed.speed())))?;
        let angle = match params.get("angle").and_then(Value::as_str) {
            Some(a) => shared.pick_angle(Some(a), None)?,
            None => p.angle.clone(),
        };
        let clip = shared.clip(&p.window, &angle)?;
        let landing = p.feed.position_at(p.feed.position(), speed);
        p.feed.replace(clip, speed);
        let p = self.playing.as_mut().expect("checked above");
        p.angle = angle.clone();
        self.params.insert("angle".into(), toml::Value::String(angle));
        self.params.insert("speed".into(), toml::Value::Float(speed));
        self.retime();
        Ok(json!({ "position_ms": landing / 1_000_000, "state": self.state() }))
    }

    /// The length and the mute follow the speed.
    fn retime(&self) {
        let Some(p) = &self.playing else { return };
        let duration = gst::ClockTime::from_nseconds(p.feed.duration());
        for src in &p.sources {
            src.set_duration(duration);
        }
        p.volume.set_property("mute", p.feed.muted());
    }
}

fn toml_of(v: &Value) -> Option<toml::Value> {
    Some(match v {
        Value::String(s) => toml::Value::String(s.clone()),
        Value::Number(n) => toml::Value::Float(n.as_f64()?),
        _ => return None,
    })
}
