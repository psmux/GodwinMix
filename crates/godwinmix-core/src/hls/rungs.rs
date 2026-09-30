//! A ladder's rungs, packaged from what the rendition planner made.
//!
//! ```text
//! programme pipeline                           the output's own pipeline
//! rung 0 tee ─▶ the core's feed ─▶ proxysink ─▶ proxysrc ─▶ packager (720p)
//! rung 1 tee ─▶ leaky queue    ─▶ proxysink ─▶ proxysrc ─▶ packager (480p)
//! rung 2 tee ─▶ leaky queue    ─▶ proxysink ─▶ proxysrc ─▶ packager (360p)
//! ```
//!
//! The top rung comes down the feed every output has, so it rides out a
//! rebuild the same way. Each lower rung gets a feed of its own off its tee
//! ([`Tap::feed`]), video only, since the sound every rung shares is the top
//! rung's. The feeds are made again for every generation of the output's
//! pipeline, because a `proxysink` that served one `proxysrc` does not replay
//! the caps to the next. The packagers live in the output's pipeline, so a
//! muxer that fails posts on the output's bus and never the programme's.

use super::request::rung_slug;
use super::stream::Stream;
use super::track::TrackKind;
use super::{attach, Input};
use crate::gstutil::make;
use crate::plugin::output::OutputCtx;
use crate::render::{Feed, Tap};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::Arc;

/// The feeds of one generation.
#[derive(Default)]
pub struct Rungs {
    feeds: Vec<Feed>,
}

impl Rungs {
    /// Package every rung of `ctx.taps`: the top one from `top`, the pad the
    /// core's feed arrives on, and the rest from feeds of their own.
    pub fn build(&mut self, ctx: &OutputCtx<'_>, stream: &Arc<Stream>, top: &gst::Pad) -> Result<()> {
        self.detach();
        let Some((first, rest)) = ctx.taps.split_first() else { return Ok(()) };
        let id = slug(ctx.id, first);
        attach(ctx.pipeline, stream, Input { id: &id, kind: TrackKind::Video, pad: top, declared_kbps: kbps(first) })?;
        for tap in rest {
            self.feed(ctx, stream, tap)?;
        }
        Ok(())
    }

    /// Take this generation's feeds out of the programme pipeline.
    pub fn detach(&mut self) {
        for feed in self.feeds.drain(..) {
            feed.detach();
        }
    }

    fn feed(&mut self, ctx: &OutputCtx<'_>, stream: &Arc<Stream>, tap: &Tap) -> Result<()> {
        let id = slug(ctx.id, tap);
        let name = format!("hls-{}-{id}-g{}", ctx.id, ctx.generation);
        let video_only = Tap { audio_tee: None, ..tap.clone() };
        let feed = video_only.feed(&name, 2.0).with_context(|| format!("feeding rung {id} to {}", ctx.id))?;
        let proxy = feed.video.clone();
        self.feeds.push(feed);
        let proxy = proxy.with_context(|| format!("rung {id} of {} has no picture to package", ctx.id))?;
        let src = make("proxysrc", &format!("{name}-src"))?;
        src.set_property("proxysink", &proxy);
        ctx.pipeline.add(&src).context("adding a rung's proxysrc")?;
        let pad = src.static_pad("src").context("proxysrc has no src pad")?;
        attach(ctx.pipeline, stream, Input { id: &id, kind: TrackKind::Video, pad: &pad, declared_kbps: kbps(tap) })?;
        Ok(())
    }
}

/// A rung's name in its URLs.
fn slug(output: &str, tap: &Tap) -> String {
    rung_slug(output, &tap.request, tap.video.as_ref().map(|v| v.height))
}

/// What a rung's picture asked for, for `BANDWIDTH` before anything is
/// measured. The sound is its own track and adds its own.
fn kbps(tap: &Tap) -> u32 {
    tap.video.as_ref().map_or(0, |v| v.bitrate_kbps)
}
