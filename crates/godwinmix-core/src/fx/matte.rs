//! A luma matte wipe: the new scene shows where the matte is darker than
//! the progress, with a soft edge.
//!
//! The matte is read once, on a thread of its own, into one byte a pixel at
//! the canvas size, and kept for the next take of the same file. Each frame
//! the progress becomes a table of 256 weights, so a pixel costs a lookup
//! and a mix: no division, no float. Until the matte has been read (the
//! first take of a file, for the few milliseconds a PNG takes) the wipe is a
//! dissolve.

use super::frame::Pic;
use super::Mix;
use crate::overlay::blend::Planes;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, OnceLock};

type Plane = Arc<OnceLock<Vec<u8>>>;

/// Mattes already read, by file and size.
type Read = Mutex<HashMap<(PathBuf, i32, i32), Plane>>;
static READ: LazyLock<Read> = LazyLock::new(Mutex::default);

pub struct Matte {
    plane: Plane,
    width: usize,
    /// Edge width as a fraction of the 0 to 1 range, and white first.
    soft: f64,
    invert: bool,
}

impl Matte {
    /// The matte in `path` at `size`, read in the background the first time.
    pub fn load(path: &Path, size: (i32, i32), soft: f64, invert: bool) -> Matte {
        let key = (path.to_path_buf(), size.0, size.1);
        let mut read = READ.lock();
        let plane = read.entry(key).or_default().clone();
        if read.len() > 16 {
            read.retain(|_, p| Arc::strong_count(p) > 1);
        }
        drop(read);
        if plane.get().is_none() {
            let (target, path) = (plane.clone(), path.to_path_buf());
            let _ = std::thread::Builder::new().name("gmx-fx-matte".into()).spawn(move || match decode(&path, size) {
                Ok(bytes) => {
                    let _ = target.set(bytes);
                }
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "a luma matte would not read; the wipe is a dissolve"),
            });
        }
        Matte { plane, width: size.0 as usize, soft: soft.clamp(0.0, 1.0), invert }
    }

    /// A matte already read, width bytes to a row: for the preview strip.
    pub fn from_plane(plane: Vec<u8>, width: usize, soft: f64, invert: bool) -> Matte {
        let cell: Plane = Arc::default();
        let _ = cell.set(plane);
        Matte { plane: cell, width, soft: soft.clamp(0.0, 1.0), invert }
    }

    /// How much of the old picture each matte value keeps, at `t`.
    fn table(&self, t: f64) -> [u8; 256] {
        let s = self.soft.max(1.0 / 255.0);
        let edge = t * (1.0 + s) - s;
        let mut out = [0u8; 256];
        for (m, w) in out.iter_mut().enumerate() {
            let m = if self.invert { 255 - m } else { m } as f64 / 255.0;
            let x = ((m - edge) / s).clamp(0.0, 1.0);
            *w = (x * x * (3.0 - 2.0 * x) * 255.0).round() as u8;
        }
        out
    }
}

fn mix8(over: u8, under: u8, a: u8) -> u8 {
    ((over as u32 * a as u32 + under as u32 * (255 - a as u32) + 127) / 255) as u8
}

impl Mix for Matte {
    fn mix(&self, old: &Pic<'_>, f: &mut Planes<'_>, t: f64) {
        let Some(m) = self.plane.get() else { return dissolve(old, f, t) };
        let lut = self.table(t);
        let (w, h) = (f.width as usize, f.height as usize);
        if m.len() < self.width * h {
            return dissolve(old, f, t);
        }
        for y in 0..h {
            let (dst, src) = (y * f.strides[0], y * old.strides[0]);
            for x in 0..w {
                let a = lut[m[y * self.width + x] as usize];
                if a != 0 {
                    f.y[dst + x] = if a == 255 { old.y[src + x] } else { mix8(old.y[src + x], f.y[dst + x], a) };
                }
            }
        }
        for cy in 0..h / 2 {
            for cx in 0..w / 2 {
                let a = lut[m[cy * 2 * self.width + cx * 2] as usize];
                if a == 0 {
                    continue;
                }
                let (du, dv) = (cy * f.strides[1] + cx, cy * f.strides[2] + cx);
                let (su, sv) = (cy * old.strides[1] + cx, cy * old.strides[2] + cx);
                f.u[du] = mix8(old.u[su], f.u[du], a);
                f.v[dv] = mix8(old.v[sv], f.v[dv], a);
            }
        }
    }
}

/// The old picture over the new at `1 - t`: what a matte not yet read, or a
/// shader with nothing to run it, does instead.
pub fn dissolve(old: &Pic<'_>, f: &mut Planes<'_>, t: f64) {
    let a = ((1.0 - t).clamp(0.0, 1.0) * 255.0).round() as u8;
    let (w, h) = (f.width as usize, f.height as usize);
    for (plane, rows, cols) in [(0, h, w), (1, h / 2, w / 2), (2, h / 2, w / 2)] {
        let (dst, src, ds, ss) = match plane {
            0 => (&mut *f.y, old.y, f.strides[0], old.strides[0]),
            1 => (&mut *f.u, old.u, f.strides[1], old.strides[1]),
            _ => (&mut *f.v, old.v, f.strides[2], old.strides[2]),
        };
        for r in 0..rows {
            for c in 0..cols {
                dst[r * ds + c] = mix8(src[r * ss + c], dst[r * ds + c], a);
            }
        }
    }
}

/// One picture as grey bytes at `size`, through GStreamer, so a matte can be
/// anything it decodes: PNG, JPEG, TIFF, or the first frame of a clip.
pub fn decode(path: &Path, size: (i32, i32)) -> Result<Vec<u8>> {
    let uri = gst::glib::filename_to_uri(path, None).context("the matte's path is not a file")?;
    let desc = format!(
        "uridecodebin name=dec uri=\"{uri}\" ! videoconvert ! videoscale ! video/x-raw,format=GRAY8,width={},height={},pixel-aspect-ratio=1/1 ! appsink name=out sync=false max-buffers=1",
        size.0, size.1
    );
    let pipeline = gst::parse::launch(&desc)?.downcast::<gst::Pipeline>().map_err(|_| anyhow::anyhow!("not a pipeline"))?;
    if let Some(dec) = pipeline.by_name("dec") {
        super::player::software_only(&dec);
    }
    let sink = pipeline.by_name("out").context("no sink")?.downcast::<gst_app::AppSink>().map_err(|_| anyhow::anyhow!("not an appsink"))?;
    pipeline.set_state(gst::State::Playing)?;
    let sample = sink.try_pull_preroll(gst::ClockTime::from_seconds(10));
    let _ = pipeline.set_state(gst::State::Null);
    let sample = sample.context("the matte gave no picture in ten seconds")?;
    let info = gstreamer_video::VideoInfo::from_caps(sample.caps().context("no caps")?)?;
    let map = sample.buffer().context("no buffer")?.map_readable()?;
    let stride = info.stride()[0] as usize;
    let (w, h) = (size.0 as usize, size.1 as usize);
    Ok((0..h).flat_map(|y| map[y * stride..y * stride + w].to_vec()).collect())
}
