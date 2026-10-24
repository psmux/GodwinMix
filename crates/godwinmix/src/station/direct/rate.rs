//! A direct show's frame rate, as the station plans its renditions on it.
//!
//! The host's `direct.input` carries the hub's reading of the last second
//! or less, and the host sends that event again whenever the input's state
//! or error moves: a sender restarted, the input read again from the start.
//! The reading then is of the second around the restart, 1 or 3 fps for a
//! 30 fps feed. Taken as it came, it replaced the rate the stats had
//! settled, and a rendition planned at 30 fps was planned again at 1 and
//! sat waiting (a Linux runner, 2026-10-11). So the settled rate is kept
//! across those events for as long as the picture keeps its codec and size,
//! and only the stats move it.

use super::seen::Seen;
use serde_json::{json, Value};

/// What a settled rate belongs to: the picture's codec and size.
fn picture(video: &Value) -> Value {
    json!([video["codec"], video["width"], video["height"]])
}

/// What a plan is made from, without the rates that move every second: the
/// state, the picture, the sound, every further track, and where it is read.
fn shape(input: &Value) -> Value {
    let steady = |v: &Value| {
        let mut v = v.clone();
        if let Some(o) = v.as_object_mut() {
            o.remove("kbps");
        }
        v
    };
    let tracks: Vec<Value> = input["tracks"].as_array().into_iter().flatten().map(steady).collect();
    json!([input["state"], steady(&input["video"]), steady(&input["audio"]), tracks, input["relay"], input["stream"]])
}

impl Seen {
    /// Take a `direct.input`, with the settled rate in place of the hub's
    /// reading while the picture is the one it was settled for. True when
    /// what the show's renditions are planned on moved.
    pub fn take_input(&mut self, v: &Value) -> bool {
        let mut v = v.clone();
        match self.settled.as_ref() {
            Some((for_picture, fps)) if *for_picture == picture(&v["video"]) => v["video"]["fps"] = json!(fps),
            Some(_) if v["video"].is_object() => self.settled = None,
            _ => {}
        }
        let moved = self.input.as_ref().map(shape) != Some(shape(&v));
        self.input = Some(v);
        moved
    }

    /// Take the rate the stats measure once the input has run a few
    /// seconds. `direct.input` carries the hub's first reading, of part of
    /// a second (none, or 8 for a 30 fps feed), and is not sent again for a
    /// rate. A fifth off or more is taken; a wobble is not. `STEADY`
    /// readings in a row must agree first: the seconds around a restarted
    /// sender read as 1 fps, or twice in a row as 10 fps for a 30 fps feed
    /// on a CI runner, and a rendition planned on that would carry it from
    /// then on. True when taken.
    pub fn settle_fps(&mut self) -> bool {
        const STEADY: u32 = 5;
        let measured = self.input_stats.as_ref().map(|s| s.fps).filter(|f| *f > 0.0);
        self.fps_run = match (measured, self.fps_run) {
            (Some(now), Some((then, n))) if (now - then).abs() < now / 5.0 => Some((then, n + 1)),
            (now, _) => now.map(|f| (f, 1)),
        };
        let agreed = self.fps_run.is_some_and(|(_, n)| n >= STEADY);
        let Some((input, fps)) = self.input.as_mut().zip(measured.filter(|_| agreed)) else { return false };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        if input["state"] != "live" || now.saturating_sub(input["since_ms"].as_u64().unwrap_or(now)) < 3000 {
            return false;
        }
        let video = &mut input["video"];
        if !video.is_object() {
            return false;
        }
        // A rate already within a fifth is kept as it is, and is settled too.
        let kept = video["fps"].as_f64().unwrap_or(0.0);
        let close = (kept - fps).abs() < fps / 5.0;
        if !close {
            video["fps"] = json!(fps);
        }
        self.settled = Some((picture(video), if close { kept } else { fps }));
        !close
    }
}

#[cfg(test)]
#[path = "rate_tests.rs"]
mod tests;
