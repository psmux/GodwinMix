//! What `gmxbussink` does with caps and buffers.

use gst_base::subclass::prelude::*;
use gstreamer as gst;
use gstreamer_base as gst_base;
use gstreamer_video as gst_video;

use super::imp::{BusSink, Kind};
use crate::gst::{audio_layout_of, layout_of};
use crate::{BusName, Layout, Publisher, PublisherOptions, Registry};

impl BaseSinkImpl for BusSink {
    fn set_caps(&self, caps: &gst::Caps) -> Result<(), gst::LoggableError> {
        let (layout, kind) = kind_of(caps).map_err(|e| gst::loggable_error!(gst::CAT_RUST, "{e}"))?;
        let mut state = self.state.lock().unwrap();
        if let Some((p, old)) = state.as_mut() {
            p.set_layout(layout)
                .map_err(|e| gst::loggable_error!(gst::CAT_RUST, "{e}"))?;
            *old = kind;
            return Ok(());
        }
        let p = self
            .publisher(layout)
            .map_err(|e| gst::loggable_error!(gst::CAT_RUST, "{e}"))?;
        *state = Some((p, kind));
        Ok(())
    }

    fn render(&self, buffer: &gst::Buffer) -> Result<gst::FlowSuccess, gst::FlowError> {
        let mut state = self.state.lock().unwrap();
        let Some((p, kind)) = state.as_mut() else {
            return Err(gst::FlowError::NotNegotiated);
        };
        let pushed = match kind {
            Kind::Video(info) => p.push_buffer(buffer, info).map(|_| ()),
            Kind::Audio(layout) => {
                push_sound(p, buffer, layout);
                Ok(())
            }
        };
        pushed.map_err(|e| {
            gst::element_imp_error!(self, gst::StreamError::Format, ["{e}"]);
            gst::FlowError::Error
        })?;
        Ok(gst::FlowSuccess::Ok)
    }

    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        self.state.lock().unwrap().take();
        Ok(())
    }
}

impl BusSink {
    fn publisher(&self, layout: Layout) -> Result<Publisher, crate::Error> {
        let s = self.settings.lock().unwrap();
        let name: BusName = s.name.parse()?;
        let reg = if s.dir.is_empty() {
            Registry::from_env()?
        } else {
            Registry::new(&s.dir)?
        };
        let opts = PublisherOptions {
            max_readers: s.max_readers as usize,
            leases_per_reader: s.leases as usize,
            checksum: false,
        };
        Publisher::create(&reg, &name, layout, opts)
    }
}

/// Pictures or sound, from the caps.
fn kind_of(caps: &gst::Caps) -> Result<(Layout, Kind), crate::Error> {
    let s = caps.structure(0).ok_or_else(|| crate::Error::BadLayout("empty caps".into()))?;
    if s.name().starts_with("audio/") {
        let layout = audio_layout_of(s)?;
        return Ok((layout, Kind::Audio(layout)));
    }
    let info = gst_video::VideoInfo::from_caps(caps)
        .map_err(|_| crate::Error::BadLayout("caps without a video format".into()))?;
    Ok((layout_of(&info)?, Kind::Video(info)))
}

/// Publish a buffer of sound as chunks no longer than a slot, each stamped
/// with where it starts. A chunk that finds every slot leased is dropped and
/// counted, like a picture; the owner never waits.
fn push_sound(p: &mut Publisher, buffer: &gst::Buffer, layout: &Layout) {
    let Ok(map) = buffer.map_readable() else { return };
    let frame = layout.frame_bytes() as usize;
    let max = (layout.size as usize / frame.max(1)) * frame;
    let mut at = buffer.pts().map(|t| t.nseconds());
    for chunk in map.chunks(max.max(frame)) {
        let len = chunk.len() - chunk.len() % frame.max(1);
        if len == 0 {
            continue;
        }
        let dur = layout.duration_ns(len);
        p.write_len(at, Some(dur), len as u64, |dst| dst[..len].copy_from_slice(&chunk[..len]));
        at = at.map(|t| t + dur);
    }
}
