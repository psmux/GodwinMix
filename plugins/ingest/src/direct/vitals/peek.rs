//! A channel stream's picture, for the Channels panel and the wall.
//!
//! A channel stream is not a show, so nothing watches it until somebody asks
//! for its picture. The first `channel.thumbnail` puts a tap on its hub slot,
//! with every alarm off, and the tap decodes keyframes only, about one a
//! second, as a show's does. Each ask keeps it for `ASKED_FOR_MS` more; once
//! that passes with no ask, the ticker takes the tap away and its hub reader
//! with it. A channel nobody looks at costs nothing.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use godwinmix_protocol::health::Thresholds;
use serde_json::{json, Value};

use super::calls::base64;
use super::registry::{Vitals, ASKED_FOR_MS};
use super::show::{lock, Show, Thumb};
use super::tap::Tap;
use super::now_ms;

/// The key a looked at stream is kept under, beside the shows. A show id is
/// a slug and has no colon, so the two never meet.
pub fn key(app: &str, stream: &str) -> String {
    format!("channel:{app}/{stream}")
}

impl Vitals {
    /// The newest picture of `stream` in `app`, and pictures kept coming for
    /// `ASKED_FOR_MS` more. `Err` when nothing is publishing to it; `Ok(None)`
    /// while the first keyframe is on its way.
    pub fn peek(&self, app: &str, stream: &str, width: Option<u32>) -> Result<Option<Thumb>, String> {
        let id = key(app, stream);
        let now = now_ms();
        let mut taps = lock(&self.taps);
        if !taps.contains_key(&id) {
            if !self.hub.is_live(app, stream) {
                return Err(format!("nothing is publishing to {app}/{stream}"));
            }
            let show = Arc::new(Show::new(&id, Thresholds::default(), now));
            show.set_checks(false, &Thresholds::default());
            let mut tap = Tap::new(show, app, stream);
            tap.peek = true;
            taps.insert(id.clone(), tap);
        }
        let show = taps[&id].show.clone();
        drop(taps);
        show.asked_until.store(now + ASKED_FOR_MS, Ordering::Relaxed);
        if let Some(w) = width {
            show.jpeg_width.store(w.clamp(16, 640) & !1, Ordering::Relaxed);
        }
        let thumb = lock(&show.thumb).clone();
        Ok(thumb)
    }

    /// Let go of every looked at stream nobody has asked about for
    /// `ASKED_FOR_MS`, and of its hub reader.
    pub(super) fn forget_peeks(&self, now: u64) {
        lock(&self.taps).retain(|_, t| !t.peek || t.show.wants_jpeg(now));
    }

    /// How many channel streams are being looked at now.
    pub fn peeking(&self) -> usize {
        lock(&self.taps).values().filter(|t| t.peek).count()
    }

    /// `channel.thumbnail {app, stream, width?}`: `{jpeg, width, height,
    /// at_ms}` with the JPEG in base64, `{pending: true}` while the first
    /// keyframe is on its way, or `{status: 404, why}` when nothing is live.
    pub(super) fn peek_call(&self, params: &Value) -> Value {
        let app = params["app"].as_str().unwrap_or_default();
        let stream = params["stream"].as_str().unwrap_or("main");
        let width = params["width"].as_u64().map(|w| w as u32);
        match self.peek(app, stream, width) {
            Ok(Some(t)) => json!({"jpeg": base64(&t.jpeg), "width": t.width, "height": t.height, "at_ms": t.at_ms}),
            Ok(None) => json!({"pending": true}),
            Err(why) => json!({"status": 404, "why": why}),
        }
    }
}
