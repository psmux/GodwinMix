//! What a replay source reads from its address and params, and the appsrcs it
//! plays into.

use super::replay::ReplaySource;
use crate::config::Params;
use crate::replay::Shared;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer_app as gst_app;
use std::sync::Arc;

/// The angle a `replay://cam1` address names.
pub(super) fn angle_of_uri(uri: &str) -> Option<String> {
    let rest = uri.trim().get("replay://".len()..)?;
    let name = rest.split(['?', '#', '/']).next().unwrap_or("");
    (!name.is_empty()).then(|| name.to_string())
}

pub(super) fn text(p: &Params, key: &str) -> Option<String> {
    p.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

pub(super) fn number(p: &Params, key: &str) -> Option<f64> {
    p.get(key).and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
}

impl ReplaySource {
    pub(super) fn shared(&self) -> Result<Arc<Shared>> {
        crate::replay::registry::find(self.ctx.origin)
            .context("replay is not running on this mixer. Turn it on with replay.start, then add the replay source again")
    }

    /// The window and clip these params ask for.
    pub(super) fn resolve(&self, p: &Params) -> Result<(crate::replay::Window, crate::replay::clip::Clip, f64)> {
        let shared = self.shared()?;
        let speed = crate::replay::shared::check_speed(number(p, "speed"))?;
        let angle = text(p, "angle").or_else(|| angle_of_uri(&self.ctx.cfg.uri));
        let window = shared.window(text(p, "event").as_deref(), number(p, "seconds"), angle.as_deref())?;
        let clip = shared.clip(&window, &window.angle)?;
        Ok((window, clip, speed))
    }
}

/// One of the two appsrcs: clip times, seekable, never live.
pub(super) fn appsrc(name: &str, caps: Option<gst::Caps>, duration: u64) -> gst_app::AppSrc {
    let src = gst_app::AppSrc::builder()
        .name(name)
        .format(gst::Format::Time)
        .stream_type(gst_app::AppStreamType::Seekable)
        .is_live(false)
        .build();
    src.set_caps(caps.as_ref());
    src.set_duration(gst::ClockTime::from_nseconds(duration));
    src
}

