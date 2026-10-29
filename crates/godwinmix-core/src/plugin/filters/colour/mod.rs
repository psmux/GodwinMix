//! `color/filter`: brightness, contrast, gamma, saturation and hue.
//!
//! The bin is one `identity` with a probe that changes the I420 frame in
//! place through two tables (`tables`), so a correction costs one read per
//! sample and nothing at all while every setting is 0. It goes wherever a
//! video filter goes: on a scene item, on a source, or on the programme. An
//! OBS `color_filter` comes across as this one with its settings; its
//! `opacity`, `color_add` and `color_multiply` are kept in the document and
//! not applied yet.

mod params;
mod tables;
#[cfg(test)]
mod tests;

pub use params::Settings;

use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::plugin::filter::{Filter, Stream};
use crate::plugin::{
    CapabilitySet, Configure, Manifest, MediaDecl, ProvideKind, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use std::sync::Arc;
use tables::Tables;

pub const MANIFEST: Manifest = Manifest {
    plugin: "color",
    id: "filter",
    kind: ProvideKind::Filter,
    api: API_LEVEL,
    description: "Brightness, contrast, gamma, saturation and hue",
    uri_schemes: &[],
    rank: 128,
    media: MediaDecl { video: StreamMode::Raw, audio: StreamMode::None, alpha: false, thumb: false },
    capabilities: CapabilitySet::new(),
    latency_ms: 0,
    tier: Tier::Core,
};

#[derive(Default)]
struct State {
    tables: Option<Arc<Tables>>,
    info: Option<(gst::Caps, gst_video::VideoInfo)>,
}

#[derive(Default)]
pub struct Colour {
    state: Arc<Mutex<State>>,
}

impl Colour {
    fn set(&self, params: &Params) -> Result<()> {
        let s = Settings::from_params(params)?;
        let tables = (!s.is_identity()).then(|| Arc::new(Tables::build(&s)));
        self.state.lock().tables = tables;
        Ok(())
    }
}

fn on_buffer(state: &Mutex<State>, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
    let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else { return gst::PadProbeReturn::Ok };
    let (tables, vinfo) = {
        let mut st = state.lock();
        let Some(tables) = st.tables.clone() else { return gst::PadProbeReturn::Ok };
        let Some(caps) = pad.current_caps() else { return gst::PadProbeReturn::Ok };
        if st.info.as_ref().map(|(c, _)| c != &caps).unwrap_or(true) {
            match gst_video::VideoInfo::from_caps(&caps) {
                Ok(i) if i.format() == gst_video::VideoFormat::I420 => st.info = Some((caps, i)),
                _ => return gst::PadProbeReturn::Ok,
            }
        }
        (tables, st.info.as_ref().map(|(_, i)| i.clone()).expect("set above"))
    };
    let Ok(mut frame) = gst_video::VideoFrameRef::from_buffer_ref_writable(buffer.make_mut(), &vinfo) else {
        return gst::PadProbeReturn::Ok;
    };
    let [y, u, v, _] = frame.planes_data_mut();
    tables::apply(&tables, y, u, v);
    gst::PadProbeReturn::Ok
}

impl Filter for Colour {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn build(&mut self, _canvas: &CanvasCaps, params: &Params) -> Result<gst::Element> {
        self.set(params)?;
        static BUILT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = match params.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => format!("color-{n}"),
        };
        let bin = gst::Bin::with_name(&format!("filter-{name}"));
        let stage = make("identity", &format!("filter-{name}-colour"))?;
        crate::probe::set_bool(&stage, "silent", true);
        bin.add(&stage).context("adding the colour correction")?;
        let sink = stage.static_pad("sink").context("colour correction has no sink pad")?;
        let src = stage.static_pad("src").context("colour correction has no src pad")?;
        let state = self.state.clone();
        src.add_probe(gst::PadProbeType::BUFFER, move |pad, info| on_buffer(&state, pad, info))
            .context("watching the colour correction's frames")?;
        bin.add_pad(&gst::GhostPad::with_target(&sink)?).context("ghosting the sink pad")?;
        bin.add_pad(&gst::GhostPad::with_target(&src)?).context("ghosting the src pad")?;
        Ok(bin.upcast())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        self.set(params)?;
        Ok(Configure::Applied)
    }

    fn stream(&self) -> Stream {
        Stream::Video
    }
}

/// A bad param names the field and its range.
pub fn validate(params: &Params) -> Result<()> {
    Settings::from_params(params).map(|_| ())
}
