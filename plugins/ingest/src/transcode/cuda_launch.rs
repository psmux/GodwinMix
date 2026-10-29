//! Running the card's deinterlacer on one picture (`cuda_deint`): the frame
//! mapped for device pointers, the kernel over each plane in the memory's
//! own context and stream, and waited for.

use gstreamer as gst;
use gstreamer::glib::translate::ToGlibPtr;
use gstreamer_video as gst_video;

use super::cuda_api::{self, Api, Ptr};
use super::cuda_scratch::Scratch;

/// `GST_MAP_CUDA`: map for a device pointer rather than a copy in memory.
const MAP_CUDA: u32 = gst::ffi::GST_MAP_FLAG_LAST << 1;

/// The kernel over each plane of `buffer`, in its memory's context and
/// stream, waited for before the buffer goes on.
pub(super) fn run(buffer: &mut gst::BufferRef, info: &gst_video::VideoInfo, scratch: &mut Scratch, threshold: i32) -> Result<(), String> {
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
