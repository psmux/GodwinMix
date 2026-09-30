//! The reading side: `gmxbussrc` in front of the shared normaliser, and the
//! probes that feed `watch`.

use crate::plugin::kinds::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::plugin::MediaEnds;
use anyhow::{Context, Result};
use godwinmix_framebus::{monotonic_ns, BusName};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::path::Path;
use std::sync::Arc;

pub use super::watch::{Samples, Watch};

/// Which tracks a shared source reads.
#[derive(Clone, Copy, Debug)]
pub struct Tracks {
    pub video: bool,
    pub audio: bool,
}

/// The source's pipeline: the bus, then the normaliser every source has.
///
/// Pictures alone are stamped on arrival. With sound, both tracks keep the
/// owner's timestamps and `retime` moves them onto this pipeline together.
pub fn build(
    ctx: &BuildCtx,
    thumb: bool,
    name: &BusName,
    dir: &Path,
    tracks: Tracks,
    watch: &Arc<Watch>,
) -> Result<MediaEnds> {
    super::register_elements()?;
    let make = |bus: BusName, tag: &str| -> Result<gst::Element> {
        let src = crate::gstutil::make("gmxbussrc", &format!("{}-src-bus{tag}", ctx.id))?;
        src.set_property("bus-name", bus.to_string());
        src.set_property("bus-dir", dir.display().to_string());
        if tracks.audio {
            src.set_property("timestamps", "owner");
        }
        Ok(src)
    };
    let video = tracks.video.then(|| make(name.clone(), "")).transpose()?;
    let audio = tracks.audio.then(|| make(name.audio(), "-audio")).transpose()?;
    let all: Vec<gst::Element> = video.iter().chain(audio.iter()).cloned().collect();
    let ingest = Ingest::default().with(all.clone()).livesync(false);
    let ends = assemble(ctx, thumb, ingest, |w: &Wiring| {
        if let Some(v) = &video {
            v.link(&w.norm.video_entry()).context("linking the bus's pictures to the normaliser")?;
            w.has_video.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(a) = &audio {
            a.link(&w.norm.audio_entry()).context("linking the bus's sound to the normaliser")?;
            w.has_audio.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(KindParts::default())
    })?;
    if tracks.audio {
        let anchor = Arc::new(super::retime::Anchor::default());
        for src in &all {
            anchor.install(&src.static_pad("src").context("gmxbussrc has no src pad")?);
        }
    }
    let first = all.first().context("a shared source with neither pictures nor sound")?;
    let hop = watch.clone();
    probe(first, "src", move |at| {
        if let Some(at) = at {
            hop.hop.add_ns(monotonic_ns().saturating_sub(at));
        }
    })?;
    let out = watch.clone();
    let end = if tracks.video { &ends.video } else { &ends.audio };
    probe(end, "sink", move |at| out.frame(at))?;
    Ok(ends)
}

/// Call `f` with each buffer's publish time from the owner, when it has one.
fn probe(el: &gst::Element, pad: &str, f: impl Fn(Option<u64>) + Send + Sync + 'static) -> Result<()> {
    let pad = el.static_pad(pad).with_context(|| format!("{} has no {pad} pad", el.name()))?;
    let caps = gst::Caps::new_empty_simple(godwinmix_framebus::gst::CAPTURED_CAPS);
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        if let Some(buffer) = info.buffer() {
            let at = buffer
                .meta::<gst::ReferenceTimestampMeta>()
                .filter(|m| m.reference().can_intersect(&caps))
                .map(|m| m.timestamp().nseconds());
            f(at);
        }
        gst::PadProbeReturn::Ok
    });
    Ok(())
}
