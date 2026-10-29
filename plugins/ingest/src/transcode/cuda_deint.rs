//! Deinterlacing on the card, for a CUDA scale (`deinterlace.cu`).
//!
//! The stage is the picture converted to I420 on the card, which makes a
//! buffer of its own, and a probe on it that runs the kernel on each plane
//! and says the picture is progressive. Nothing leaves the card. Before
//! this, the picture went down to system memory for `deinterlace`, which on
//! a headend of fifty channels was nine cores of YADIF and five more of
//! copies.
//!
//! One picture out for each picture in, the top field kept, as the CPU
//! stage was set (`fields=top`). `linear` rebuilds every line of the other
//! field; `yadif` and `greedyh` keep still detail woven and rebuild only
//! where the fields comb.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;

use super::cuda_api::{self, Api, Ptr};
use super::cuda_scratch::Scratch;

/// How far outside its neighbours a sample may sit and still be detail.
const THRESHOLD: i32 = 10;

/// `GST_MAP_CUDA`: map for a device pointer rather than a copy in memory.
const MAP_CUDA: u32 = gst::ffi::GST_MAP_FLAG_LAST << 1;

/// The flags that say a buffer holds fields.
const FIELDS: gst_video::VideoBufferFlags = gst_video::VideoBufferFlags::INTERLACED
    .union(gst_video::VideoBufferFlags::TFF)
    .union(gst_video::VideoBufferFlags::RFF)
    .union(gst_video::VideoBufferFlags::ONEFIELD);

/// Whether this machine can deinterlace on the card: the CUDA calls are
/// there. The kernel itself loads with the first picture.
pub fn available() -> bool {
    cuda_api::api().is_ok()
}

/// What the stage knows from its caps, and the copy it reads from.
struct State {
    info: Option<gst_video::VideoInfo>,
    mixed: bool,
    scratch: Scratch,
    failed: bool,
}

/// Run the kernel on every picture through `pad` that holds fields, all of
/// them when `always`, and say downstream that they are progressive.
pub fn install(pad: &gst::Pad, always: bool, linear: bool) {
    let state = Arc::new(Mutex::new(State { info: None, mixed: false, scratch: Scratch::default(), failed: false }));
    let caps_state = state.clone();
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        let Some(gst::PadProbeData::Event(event)) = &info.data else { return gst::PadProbeReturn::Ok };
        let gst::EventView::Caps(c) = event.view() else { return gst::PadProbeReturn::Ok };
        let Some(progressive) = progressive(c.caps(), &mut caps_state.lock().unwrap_or_else(|p| p.into_inner())) else {
            return gst::PadProbeReturn::Ok;
        };
        info.data = Some(gst::PadProbeData::Event(gst::event::Caps::new(&progressive)));
        gst::PadProbeReturn::Ok
    });
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        let mut st = state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(buffer) = info.buffer_mut() {
            deinterlace(buffer, &mut st, always, linear);
        }
        gst::PadProbeReturn::Ok
    });
}

/// Note the picture's layout and the caps' interlace mode, and give back
/// the caps made progressive, or `None` for caps that are already.
fn progressive(caps: &gst::CapsRef, st: &mut State) -> Option<gst::Caps> {
    st.info = gst_video::VideoInfo::from_caps(caps).ok();
    let mode = caps.structure(0)?.get::<&str>("interlace-mode").unwrap_or("progressive");
    st.mixed = mode == "mixed";
    if mode == "progressive" {
        return None;
    }
    let mut changed = caps.to_owned();
    let s = changed.make_mut().structure_mut(0)?;
    s.set("interlace-mode", "progressive");
    s.remove_field("field-order");
    Some(changed)
}

fn deinterlace(buffer: &mut gst::BufferRef, st: &mut State, always: bool, linear: bool) {
    let fields = !st.mixed || always || buffer.flags().contains(gst::BufferFlags::from_bits_truncate(gst_video::VideoBufferFlags::INTERLACED.bits()));
    let Some(info) = st.info.clone() else { return };
    if fields {
        if let Err(e) = run(buffer, &info, &mut st.scratch, if linear { -1 } else { THRESHOLD }) {
            if !std::mem::replace(&mut st.failed, true) {
                eprintln!("transcode: deinterlacing on the card failed, these pictures stay woven: {e}");
            }
        }
    }
    buffer.unset_flags(gst::BufferFlags::from_bits_truncate(FIELDS.bits()));
}

/// The kernel over each plane of `buffer`, in its memory's context and
/// stream, waited for before the buffer goes on.
fn run(buffer: &mut gst::BufferRef, info: &gst_video::VideoInfo, scratch: &mut Scratch, threshold: i32) -> Result<(), String> {
    let api = cuda_api::api()?;
    let planes = planes(info)?;
    let mem = buffer.peek_memory(0).as_ptr() as *mut gst::ffi::GstMemory;
    // SAFETY: the memory is checked to be CUDA memory before its context is
    // read; the frame is mapped for writing with the CUDA flag, so its plane
    // pointers are device pointers in that context, and it is unmapped on
    // every path; the context is pushed for the calls and popped after.
    unsafe {
        if buffer.n_memory() != 1 || (api.is_cuda)(mem) == 0 {
            return Err("the picture is not in CUDA memory".into());
        }
        let ctx = (*(mem as *mut CudaMemory)).context;
        if (api.push)(ctx) == 0 {
            return Err("the CUDA context would not go current".into());
        }
        let mut frame = std::mem::MaybeUninit::<gst_video::ffi::GstVideoFrame>::zeroed();
        let flags = gst::ffi::GST_MAP_READWRITE | MAP_CUDA;
        let result = if gst_video::ffi::gst_video_frame_map(frame.as_mut_ptr(), info.to_glib_none().0, buffer.as_mut_ptr(), flags) == 0 {
            Err("the picture would not map on the card".to_string())
        } else {
            let frame = frame.assume_init_mut();
            let stream = (api.stream_handle)((api.stream)(mem));
            let r = planes_run(api, frame, &planes, stream, scratch, ctx, threshold);
            gst_video::ffi::gst_video_frame_unmap(frame);
            r
        };
        (api.pop)(std::ptr::null_mut());
        result
    }
}

/// Each plane's width in bytes, its height in lines and its sample step.
fn planes(info: &gst_video::VideoInfo) -> Result<Vec<(usize, usize, i32)>, String> {
    let (w, h) = (info.width() as usize, info.height() as usize);
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    match info.format() {
        gst_video::VideoFormat::I420 => Ok(vec![(w, h, 1), (cw, ch, 1), (cw, ch, 1)]),
        gst_video::VideoFormat::Nv12 => Ok(vec![(w, h, 1), (cw * 2, ch, 2)]),
        f => Err(format!("the card's deinterlacer takes I420 or NV12, not {f:?}")),
    }
}

/// # Safety
/// `frame` is mapped with the CUDA flag and `ctx` is current.
unsafe fn planes_run(
    api: &Api,
    frame: &gst_video::ffi::GstVideoFrame,
    planes: &[(usize, usize, i32)],
    stream: Ptr,
    scratch: &mut Scratch,
    ctx: Ptr,
    threshold: i32,
) -> Result<(), String> {
    let f = cuda_api::kernel(api)?;
    for (i, &(width, height, step)) in planes.iter().enumerate() {
        let pitch = frame.info.stride[i] as usize;
        let size = pitch * height;
        let src = scratch.at_least(api, ctx, size)?;
        let dst = frame.data[i] as u64;
        check((api.copy)(src, dst, size, stream), "copying the plane")?;
        let (mut s, mut d) = (src, dst);
        let (mut p, mut w, mut h, mut rebuild, mut st, mut t) = (pitch as i32, width as i32, height as i32, 1i32, step, threshold);
        let mut args: [Ptr; 8] = [&mut s as *mut u64 as Ptr, &mut d as *mut u64 as Ptr, &mut p as *mut i32 as Ptr, &mut w as *mut i32 as Ptr, &mut h as *mut i32 as Ptr, &mut rebuild as *mut i32 as Ptr, &mut st as *mut i32 as Ptr, &mut t as *mut i32 as Ptr];
        let rows = (height as u32).div_ceil(2);
        let grid = ((width as u32).div_ceil(32), rows.div_ceil(8));
        check((api.launch)(f, grid.0, grid.1, 1, 32, 8, 1, 0, stream, args.as_mut_ptr(), std::ptr::null_mut()), "starting the kernel")?;
    }
    check((api.sync)(stream), "waiting for the kernel")
}

pub(super) fn check(code: i32, what: &str) -> Result<(), String> {
    if code == 0 { Ok(()) } else { Err(format!("{what} failed (CUDA error {code})")) }
}

/// The head of `GstCudaMemory`, enough to read its context.
#[repr(C)]
struct CudaMemory {
    mem: gst::ffi::GstMemory,
    context: Ptr,
}
