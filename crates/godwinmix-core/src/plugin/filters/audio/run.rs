//! The probe every audio filter runs in.
//!
//! The bin is one `identity` with a buffer probe on its src pad, as the chroma
//! key is: the samples are changed in place on the streaming thread that
//! carries them, with no element of our own to register, no extra thread and
//! no copy beyond the one `make_mut` makes when a buffer is shared. The
//! canvas audio is S16LE interleaved; each buffer is read into a float
//! scratch kept between buffers, worked on, and written back clamped.

use crate::config::Params;
use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use tracing::warn;

/// One stage of sound processing. Samples are interleaved, -1 to 1.
pub trait Dsp: Send + 'static {
    /// Read the settings. A bad one leaves the stage as it was.
    fn set(&mut self, params: &Params) -> Result<()>;
    /// The format is known, or has changed. Clears any state kept between
    /// buffers.
    fn prepare(&mut self, rate: u32, channels: usize);
    fn process(&mut self, samples: &mut [f32], channels: usize);
    /// The delay this stage adds, in milliseconds.
    fn latency_ms(&self) -> u32 {
        0
    }
    /// Whether the stage can run at this sample rate.
    fn accepts(&self, _rate: u32) -> Result<()> {
        Ok(())
    }
}

struct Inner {
    dsp: Box<dyn Dsp>,
    format: Option<(u32, usize)>,
    scratch: Vec<f32>,
    bypass: bool,
    /// Set once the caps through the filter turn out not to be the canvas
    /// audio, so the warning is written once and the buffers pass untouched.
    refused: bool,
}

pub struct Runner {
    kind: String,
    inner: Mutex<Inner>,
}

impl Runner {
    pub fn new(kind: String, dsp: Box<dyn Dsp>) -> Runner {
        let inner = Inner { dsp, format: None, scratch: Vec::new(), bypass: false, refused: false };
        Runner { kind, inner: Mutex::new(inner) }
    }

    pub fn set(&self, params: &Params) -> Result<()> {
        let bypass = super::values::flag(&self.kind, params, "bypass", false)?;
        let mut st = self.inner.lock();
        st.dsp.set(params)?;
        st.bypass = bypass;
        Ok(())
    }

    pub fn accepts(&self, rate: u32) -> Result<()> {
        self.inner.lock().dsp.accepts(rate)
    }

    pub fn latency_ms(&self) -> u32 {
        self.inner.lock().dsp.latency_ms()
    }

    pub fn on_buffer(&self, pad: &gst::Pad, info: &mut gst::PadProbeInfo) -> gst::PadProbeReturn {
        let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        let mut st = self.inner.lock();
        if st.bypass || st.refused {
            return gst::PadProbeReturn::Ok;
        }
        let Some((rate, channels)) = format_of(pad) else {
            st.refused = true;
            warn!(filter = %self.kind, pad = %pad.name(), "an audio filter is on a stream that is not the canvas audio; it lets the stream through untouched");
            return gst::PadProbeReturn::Ok;
        };
        if st.format != Some((rate, channels)) {
            st.dsp.prepare(rate, channels);
            st.format = Some((rate, channels));
        }
        let buf = buffer.make_mut();
        let sounded = {
            let Ok(mut map) = buf.map_writable() else { return gst::PadProbeReturn::Ok };
            let Inner { dsp, scratch, .. } = &mut *st;
            run(dsp.as_mut(), scratch, map.as_mut_slice(), channels)
        };
        // A gap is silence the mixer may skip. A delay or a gate's tail can
        // put sound into one, and then it must be heard.
        if sounded && buf.flags().contains(gst::BufferFlags::GAP) {
            buf.unset_flags(gst::BufferFlags::GAP);
        }
        gst::PadProbeReturn::Ok
    }
}

/// S16LE in, worked on as floats, S16LE out. True when anything but silence
/// came out.
fn run(dsp: &mut dyn Dsp, scratch: &mut Vec<f32>, bytes: &mut [u8], channels: usize) -> bool {
    scratch.clear();
    scratch.extend(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0));
    let whole = scratch.len() - scratch.len() % channels.max(1);
    dsp.process(&mut scratch[..whole], channels);
    let mut sounded = false;
    for (b, s) in bytes.chunks_exact_mut(2).zip(scratch.iter()) {
        let v = (s * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        sounded |= v != 0;
        b.copy_from_slice(&v.to_le_bytes());
    }
    sounded
}

/// The rate and channel count, when the pad carries interleaved S16LE.
fn format_of(pad: &gst::Pad) -> Option<(u32, usize)> {
    let caps = pad.current_caps()?;
    let s = caps.structure(0)?;
    if s.name() != "audio/x-raw" || s.get::<&str>("format").ok()? != crate::caps::AUDIO_FORMAT {
        return None;
    }
    if s.get::<&str>("layout").map(|l| l != "interleaved").unwrap_or(false) {
        return None;
    }
    let rate = s.get::<i32>("rate").ok()?;
    let channels = s.get::<i32>("channels").ok()?;
    (rate > 0 && channels > 0).then_some((rate as u32, channels as usize))
}
