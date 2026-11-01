//! `lut/filter`: a look. A LUT from the library or a file, how much of it,
//! and a colour correction before it, or a saved preset of all three.
//!
//! The bin is one `identity` with a probe that changes the I420 frame in
//! place, like `color/filter`. Everything the filter is told is baked into
//! one table in Y'CbCr (`lut::bake`) and a frame only interpolates in it
//! (`lut::apply`). Nothing is set, nothing is done: with no LUT and the
//! colour all 0 the probe returns at once.
//!
//! A change on air (`configure`) checks the names at once, so a mistake is
//! answered, and bakes on a thread of its own: the old look stays on the
//! picture until the new table is ready, and the frame's thread never reads
//! a file.

mod params;
#[cfg(test)]
mod tests;

pub use params::{merged, preset_params, validate};

use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::lut::apply::{self, Planes};
use crate::lut::bake::{self, Recipe, Table};
use crate::lut::library;
use crate::plugin::filter::{Filter, Stream};
use crate::plugin::{CapabilitySet, Configure, Manifest, MediaDecl, ProvideKind, StreamMode, Tier, API_LEVEL};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub const MANIFEST: Manifest = Manifest {
    plugin: "lut",
    id: "filter",
    kind: ProvideKind::Filter,
    api: API_LEVEL,
    description: "A look: a LUT (.cube, .3dl or a PNG), a built in camera conversion or grade, or a saved preset",
    uri_schemes: &[],
    rank: 128,
    media: MediaDecl { video: StreamMode::Raw, audio: StreamMode::None, alpha: false, thumb: false },
    capabilities: CapabilitySet::new(),
    latency_ms: 0,
    tier: Tier::Core,
};

#[derive(Default)]
struct State {
    table: Option<Arc<Table>>,
    info: Option<(gst::Caps, gst_video::VideoInfo)>,
}

pub struct Look {
    state: Arc<Mutex<State>>,
    /// Which bake is the newest, so a slow one never replaces a later one.
    generation: Arc<AtomicU64>,
    dir: PathBuf,
}

impl Default for Look {
    fn default() -> Self {
        Look { state: Default::default(), generation: Default::default(), dir: library::dir() }
    }
}

/// The table for `params`, or none when it would change nothing.
pub fn table_for(params: &Params, dir: &std::path::Path) -> Result<Option<Table>> {
    let p = merged(params, dir)?;
    let lut = params::load(&params::source(&p), dir)?;
    let recipe = Recipe { lut: lut.as_deref(), amount: params::amount(&p)?, colour: params::colour(&p)? };
    Ok((!recipe.is_identity()).then(|| bake::build(&recipe)))
}

impl Look {
    #[cfg(test)]
    fn in_dir(dir: PathBuf) -> Look {
        Look { state: Default::default(), generation: Default::default(), dir }
    }

    fn set_now(&self, params: &Params) -> Result<()> {
        let n = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let table = table_for(params, &self.dir)?.map(Arc::new);
        if self.generation.load(Ordering::SeqCst) == n {
            self.state.lock().table = table;
        }
        Ok(())
    }

    /// Check the names now and bake on a thread.
    fn set_later(&self, params: &Params) -> Result<()> {
        let p = merged(params, &self.dir)?;
        params::validate(&p)?;
        if let params::Source::Named(n) = params::source(&p) {
            if crate::lut::looks::find(&n).is_none() && !self.dir.join(format!("{}.cube", crate::gallery::slug(&n))).is_file() {
                library::load(&self.dir, &n)?;
            }
        }
        let (n, gen, state, dir, params) = (self.generation.fetch_add(1, Ordering::SeqCst) + 1, self.generation.clone(), self.state.clone(), self.dir.clone(), params.clone());
        std::thread::Builder::new()
            .name("lut-bake".into())
            .spawn(move || match table_for(&params, &dir) {
                Ok(table) if gen.load(Ordering::SeqCst) == n => state.lock().table = table.map(Arc::new),
                Ok(_) => {}
                Err(e) => tracing::warn!("the look did not change: {e:#}"),
            })
            .context("starting the look's bake")?;
        Ok(())
    }
}

fn on_buffer(state: &Mutex<State>, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
    let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else { return gst::PadProbeReturn::Ok };
    let (table, vinfo) = {
        let mut st = state.lock();
        let Some(table) = st.table.clone() else { return gst::PadProbeReturn::Ok };
        let Some(caps) = pad.current_caps() else { return gst::PadProbeReturn::Ok };
        if st.info.as_ref().map(|(c, _)| c != &caps).unwrap_or(true) {
            match gst_video::VideoInfo::from_caps(&caps) {
                Ok(i) if i.format() == gst_video::VideoFormat::I420 => st.info = Some((caps, i)),
                _ => return gst::PadProbeReturn::Ok,
            }
        }
        (table, st.info.as_ref().map(|(_, i)| i.clone()).expect("set above"))
    };
    let Ok(mut frame) = gst_video::VideoFrameRef::from_buffer_ref_writable(buffer.make_mut(), &vinfo) else {
        return gst::PadProbeReturn::Ok;
    };
    let strides = vinfo.stride();
    let (width, height) = (vinfo.width() as usize, vinfo.height() as usize);
    let [y, u, v, _] = frame.planes_data_mut();
    let planes = Planes { y, u, v, y_stride: strides[0] as usize, u_stride: strides[1] as usize, v_stride: strides[2] as usize, width, height };
    apply::frame_parallel(&table, planes);
    gst::PadProbeReturn::Ok
}

impl Filter for Look {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn build(&mut self, _canvas: &CanvasCaps, params: &Params) -> Result<gst::Element> {
        // A LUT that is not there yet must not cost the scene its picture:
        // the item comes up ungraded and the reason is in the log.
        if let Err(e) = self.set_now(params) {
            tracing::warn!("the look is off until this is fixed: {e:#}");
        }
        static BUILT: AtomicU64 = AtomicU64::new(0);
        let n = BUILT.fetch_add(1, Ordering::Relaxed);
        let name = match params.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => format!("lut-{n}"),
        };
        let bin = gst::Bin::with_name(&format!("filter-{name}"));
        let stage = make("identity", &format!("filter-{name}-lut"))?;
        crate::probe::set_bool(&stage, "silent", true);
        bin.add(&stage).context("adding the look")?;
        let sink = stage.static_pad("sink").context("the look has no sink pad")?;
        let src = stage.static_pad("src").context("the look has no src pad")?;
        let state = self.state.clone();
        src.add_probe(gst::PadProbeType::BUFFER, move |pad, info| on_buffer(&state, pad, info))
            .context("watching the look's frames")?;
        bin.add_pad(&gst::GhostPad::with_target(&sink)?).context("ghosting the sink pad")?;
        bin.add_pad(&gst::GhostPad::with_target(&src)?).context("ghosting the src pad")?;
        Ok(bin.upcast())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        self.set_later(params)?;
        Ok(Configure::Applied)
    }

    fn stream(&self) -> Stream {
        Stream::Video
    }
}
