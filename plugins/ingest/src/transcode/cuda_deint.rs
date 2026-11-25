//! Deinterlacing on the card, for a picture already in CUDA memory.
//!
//! GStreamer 1.26 has no CUDA deinterlacer and its NVDEC decoders always
//! weave, so until this the GPU path took every picture down to system
//! memory for `deinterlace` (YADIF) and back up. On the ISP box that was most
//! of the plugin's CPU: with 36 live channels, 40% of the process's samples
//! were in YADIF and another 20% in the driver copying pictures down and up,
//! five to six cores in all, while the cards had room to spare.
//!
//! Here the picture stays on the card. An `identity` carries a probe that
//! takes each picture the decoder marks as interlaced, keeps the field that
//! came first, and rebuilds the other one with a small CUDA kernel compiled
//! once by NVRTC, the same compiler `cudaconvertscale` needs:
//!
//! * where nothing moved since the last picture, the woven line is kept as
//!   it is, so still detail and captions stay sharp;
//! * where something moved, the woven line is kept only when it sits
//!   between the lines above and below it, which is where it cannot comb;
//!   otherwise it is rebuilt from them along the direction whose ends agree
//!   best, so a moving slanted edge does not turn into steps.
//!
//! One picture comes out for each picture in, as YADIF did here (`fields`
//! `top`). A progressive picture passes untouched, and so does one in a
//! format other than NV12, with one warning. When CUDA or NVRTC cannot be
//! loaded, [`available`] says so and the caller builds the CPU path instead.
//! `GMX_DEINTERLACE_CPU=1` in the environment forces the CPU path.

use std::ffi::{c_char, c_int, c_uint, c_void, CString};
use std::sync::{Mutex, OnceLock};

use gstreamer as gst;
use gstreamer::glib;
use gstreamer::glib::translate::from_glib_full;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use libloading::Library;

const KERNEL: &str = r#"
extern "C" __global__ void gmx_deint(const unsigned char* cur, const unsigned char* prev, unsigned char* dst,
    int width, int height, int cur_pitch, int prev_pitch, int dst_pitch, int keep, int has_prev, int step)
{
  int x = blockIdx.x * blockDim.x + threadIdx.x;
  int y = blockIdx.y * blockDim.y + threadIdx.y;
  if (x >= width || y >= height) return;
  int b = cur[y * cur_pitch + x];
  if ((y & 1) == keep || height < 2) { dst[y * dst_pitch + x] = (unsigned char)b; return; }
  int ya = y > 0 ? y - 1 : y + 1;
  int yc = y + 1 < height ? y + 1 : y - 1;
  const unsigned char* ra = cur + ya * cur_pitch;
  const unsigned char* rc = cur + yc * cur_pitch;
  int a = ra[x], c = rc[x];
  int lo = min(a, c), hi = max(a, c);
  int best = abs(a - c), interp = (a + c + 1) >> 1;
  if (x >= step && x + step < width) {
    int d1 = abs(ra[x - step] - rc[x + step]);
    int d2 = abs(ra[x + step] - rc[x - step]);
    if (d1 < best) { best = d1; interp = (ra[x - step] + rc[x + step] + 1) >> 1; }
    if (d2 < best) { best = d2; interp = (ra[x + step] + rc[x - step] + 1) >> 1; }
  }
  interp = min(max(interp, lo), hi);
  int out = interp;
  if (has_prev) {
    int pb = prev[y * prev_pitch + x];
    int pa = prev[ya * prev_pitch + x];
    int pc = prev[yc * prev_pitch + x];
    int motion = max(abs(b - pb), (abs(a - pa) + abs(c - pc) + 1) >> 1);
    if (motion <= 6) {
      out = b;
    } else {
      int m = max(0, 12 - motion);
      out = (b >= lo - m && b <= hi + m) ? b : interp;
    }
  }
  dst[y * dst_pitch + x] = (unsigned char)out;
}
"#;

/// `GST_MAP_CUDA`: map for the device, `data` is a `CUdeviceptr`.
const MAP_CUDA: u32 = 0x1_0000 << 1;
/// GstVideoBufferFlags, the first four after GstBufferFlags' own.
const INTERLACED: u32 = 0x10_0000;
const TFF: u32 = 0x20_0000;
const RFF: u32 = 0x40_0000;
const ONEFIELD: u32 = 0x80_0000;

/// The public head of a `GstCudaMemory`.
#[repr(C)]
struct CudaMemory {
    mem: gst::ffi::GstMemory,
    context: *mut c_void,
    info: gst_video::ffi::GstVideoInfo,
}

type Launch = unsafe extern "C" fn(*mut c_void, c_uint, c_uint, c_uint, c_uint, c_uint, c_uint, c_uint, *mut c_void, *mut *mut c_void, *mut *mut c_void) -> c_int;

struct Api {
    is_cuda_memory: unsafe extern "C" fn(*mut gst::ffi::GstMemory) -> glib::ffi::gboolean,
    push: unsafe extern "C" fn(*mut c_void) -> glib::ffi::gboolean,
    pop: unsafe extern "C" fn(*mut *mut c_void) -> glib::ffi::gboolean,
    get_stream: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    stream_handle: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pool_new: unsafe extern "C" fn(*mut c_void) -> *mut gst::ffi::GstBufferPool,
    module_load: unsafe extern "C" fn(*mut *mut c_void, *const c_void) -> c_int,
    get_function: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *const c_char) -> c_int,
    launch: Launch,
    sync: unsafe extern "C" fn(*mut c_void) -> c_int,
    ptx: CString,
}

// SAFETY: the function pointers are plain C entry points and the PTX is an
// owned string; nothing here is tied to a thread.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    API.get_or_init(load).as_ref().map_err(Clone::clone)
}

/// Whether the card can deinterlace here; why not, when it cannot.
pub fn available() -> Result<(), String> {
    if std::env::var("GMX_DEINTERLACE_CPU").is_ok_and(|v| v == "1") {
        return Err("GMX_DEINTERLACE_CPU=1".into());
    }
    api().map(|_| ())
}

fn load() -> Result<Api, String> {
    // SAFETY: each symbol is looked up by its C name and called with the
    // arguments its C declaration takes; the library is leaked, so the
    // pointers stay valid for the life of the process.
    unsafe {
        let lib = Library::new("libgstcuda-1.0.so.0").map_err(|e| format!("no GStreamer CUDA library: {e}"))?;
        macro_rules! sym {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| format!("{}: {e}", $name))?
            };
        }
        let load_cuda: unsafe extern "C" fn() -> glib::ffi::gboolean = sym!("gst_cuda_load_library");
        let load_nvrtc: unsafe extern "C" fn() -> glib::ffi::gboolean = sym!("gst_cuda_nvrtc_load_library");
        let compile: unsafe extern "C" fn(*const c_char) -> *mut c_char = sym!("gst_cuda_nvrtc_compile");
        if load_cuda() == 0 {
            return Err("GStreamer could not load CUDA".into());
        }
        if load_nvrtc() == 0 {
            return Err("GStreamer could not load NVRTC".into());
        }
        let source = CString::new(KERNEL).map_err(|e| e.to_string())?;
        let ptx = compile(source.as_ptr());
        if ptx.is_null() {
            return Err("NVRTC would not compile the deinterlace kernel".into());
        }
        let owned = std::ffi::CStr::from_ptr(ptx).to_owned();
        glib::ffi::g_free(ptx as *mut c_void);
        let api = Api {
            is_cuda_memory: sym!("gst_is_cuda_memory"),
            push: sym!("gst_cuda_context_push"),
            pop: sym!("gst_cuda_context_pop"),
            get_stream: sym!("gst_cuda_memory_get_stream"),
            stream_handle: sym!("gst_cuda_stream_get_handle"),
            pool_new: sym!("gst_cuda_buffer_pool_new"),
            module_load: sym!("CuModuleLoadData"),
            get_function: sym!("CuModuleGetFunction"),
            launch: sym!("CuLaunchKernel"),
            sync: sym!("CuStreamSynchronize"),
            ptx: owned,
        };
        std::mem::forget(lib);
        Ok(api)
    }
}

/// The kernel loaded into a context, once per context (one per card).
/// Called with that context current.
fn function(api: &Api, context: *mut c_void) -> Result<*mut c_void, String> {
    static LOADED: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
    let mut loaded = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, f)) = loaded.iter().find(|(c, _)| *c == context as usize) {
        return Ok(*f as *mut c_void);
    }
    let (mut module, mut f) = (std::ptr::null_mut(), std::ptr::null_mut());
    // SAFETY: the context is current (the caller pushed it), the PTX is a
    // NUL terminated image and the name a NUL terminated string.
    unsafe {
        let r = (api.module_load)(&mut module, api.ptx.as_ptr() as *const c_void);
        if r != 0 {
            return Err(format!("CUDA would not load the deinterlace kernel ({r})"));
        }
        let r = (api.get_function)(&mut f, module, c"gmx_deint".as_ptr());
        if r != 0 {
            return Err(format!("the deinterlace kernel has no entry point ({r})"));
        }
    }
    loaded.push((context as usize, f as usize));
    Ok(f)
}

/// The scan the caps say, as the probe needs it.
#[derive(Clone, Copy, PartialEq)]
enum Scan {
    Progressive,
    /// Every picture interlaced; `true` when the bottom field comes first.
    Interleaved(bool),
    /// Each picture says for itself in its flags.
    Mixed,
}

#[derive(Default)]
struct Stage {
    /// NV12 in CUDA memory, its size, and the scan it arrives in.
    shape: Option<(gst::Caps, u32, u32, Scan)>,
    pool: Option<gst::BufferPool>,
    prev: Option<gst::Buffer>,
    warned: bool,
}

/// The elements of a deinterlace kept on the card: an `identity` whose
/// probe does the work. `always`: every picture, as `deinterlace`'s
/// `interlaced` mode; otherwise only those marked interlaced.
pub fn elements(name: String, always: bool) -> Result<Vec<gst::Element>, String> {
    let identity = gst::ElementFactory::make("identity").name(name).build().map_err(|_| "this machine has no GStreamer element `identity`".to_string())?;
    let pad = identity.static_pad("sink").ok_or("identity has no sink pad")?;
    let stage = Mutex::new(Stage::default());
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM | gst::PadProbeType::BUFFER, move |pad, info| {
        let mut stage = stage.lock().unwrap_or_else(|e| e.into_inner());
        match &info.data {
            Some(gst::PadProbeData::Event(event)) => {
                if let gst::EventView::Caps(c) = event.view() {
                    let progressive = stage.caps(c.caps(), pad);
                    info.data = Some(gst::PadProbeData::Event(gst::event::Caps::new(&progressive)));
                }
            }
            Some(gst::PadProbeData::Buffer(buffer)) => {
                let buffer = buffer.clone();
                match stage.picture(&buffer, always) {
                    Ok(Some(out)) => info.data = Some(gst::PadProbeData::Buffer(out)),
                    Ok(None) => {}
                    Err(why) => {
                        if !stage.warned {
                            stage.warned = true;
                            eprintln!("transcode: {}: {why}; its pictures pass woven", pad.parent().map(|p| p.name().to_string()).unwrap_or_default());
                        }
                    }
                }
                stage.prev = Some(buffer);
            }
            _ => {}
        }
        gst::PadProbeReturn::Ok
    });
    Ok(vec![identity])
}

impl Stage {
    /// Note the scan and size of new caps, and say progressive downstream.
    fn caps(&mut self, caps: &gst::CapsRef, pad: &gst::Pad) -> gst::Caps {
        let mut out = caps.to_owned();
        self.shape = None;
        self.pool = None;
        self.prev = None;
        let Some(s) = caps.structure(0) else { return out };
        let cuda = caps.features(0).is_some_and(|f| f.contains("memory:CUDAMemory"));
        let nv12 = s.get::<&str>("format").ok() == Some("NV12");
        let scan = match s.get::<&str>("interlace-mode").unwrap_or("progressive") {
            "progressive" => Scan::Progressive,
            "interleaved" => Scan::Interleaved(s.get::<&str>("field-order").ok() == Some("bottom-field-first")),
            _ => Scan::Mixed,
        };
        if let Some(o) = out.make_mut().structure_mut(0) {
            o.set("interlace-mode", "progressive");
            o.remove_field("field-order");
        }
        let (w, h) = (s.get::<i32>("width").unwrap_or(0), s.get::<i32>("height").unwrap_or(0));
        if cuda && nv12 && w > 0 && h > 0 {
            self.shape = Some((out.clone(), w as u32, h as u32, scan));
        } else if scan != Scan::Progressive && !self.warned {
            self.warned = true;
            eprintln!(
                "transcode: {}: the card deinterlaces NV12 in CUDA memory, and this is {caps}; its pictures pass woven",
                pad.parent().map(|p| p.name().to_string()).unwrap_or_default()
            );
        }
        out
    }

    /// The picture made progressive, or None to pass it as it is.
    fn picture(&mut self, input: &gst::Buffer, always: bool) -> Result<Option<gst::Buffer>, String> {
        let Some((caps, width, height, scan)) = self.shape.clone() else { return Ok(None) };
        let flags = input.flags().bits();
        let bottom_first = match scan {
            Scan::Progressive if !always => return Ok(None),
            Scan::Mixed if !always && flags & INTERLACED == 0 => return Ok(None),
            Scan::Interleaved(bff) => bff,
            _ => flags & TFF == 0 && flags & INTERLACED != 0,
        };
        let api = api()?;
        if input.n_memory() != 1 {
            return Err(format!("a picture in {} pieces", input.n_memory()));
        }
        let mem = input.peek_memory(0).as_mut_ptr();
        // SAFETY: a GstMemory pointer from a live buffer.
        if unsafe { (api.is_cuda_memory)(mem) } == 0 {
            return Err("a picture not in CUDA memory".into());
        }
        let cmem = mem as *mut CudaMemory;
        // SAFETY: gst_is_cuda_memory said this is a GstCudaMemory.
        let context = unsafe { (*cmem).context };
        if self.pool.is_none() {
            self.pool = Some(pool(api, context, &caps)?);
        }
        let pool = self.pool.as_ref().ok_or("no pool")?;
        let mut out = pool.acquire_buffer(None).map_err(|e| format!("no picture from the pool: {e:?}"))?;
        {
            let o = out.get_mut().ok_or("the pool's picture is not writable")?;
            input.copy_into(o, gst::BufferCopyFlags::TIMESTAMPS | gst::BufferCopyFlags::FLAGS, ..).map_err(|e| e.to_string())?;
            o.unset_flags(gst::BufferFlags::from_bits_retain(INTERLACED | TFF | RFF | ONEFIELD));
        }
        let prev = self
            .prev
            .as_ref()
            .filter(|p| p.n_memory() == 1)
            .map(|p| p.peek_memory(0).as_mut_ptr())
            // SAFETY: a GstMemory pointer from a live buffer.
            .filter(|m| unsafe { (api.is_cuda_memory)(*m) } != 0)
            .map(|m| m as *mut CudaMemory)
            // SAFETY: checked to be CUDA memory just above.
            .filter(|m| unsafe { (**m).info.width == width as i32 && (**m).info.height == height as i32 && (**m).context == context });
        let omem = out.peek_memory(0).as_mut_ptr() as *mut CudaMemory;
        // SAFETY: the buffers stay alive across the call; each memory is
        // mapped for the device and unmapped before return.
        unsafe { run(api, context, cmem, prev, omem, width, height, if bottom_first { 1 } else { 0 })? };
        Ok(Some(out))
    }
}

fn pool(api: &Api, context: *mut c_void, caps: &gst::Caps) -> Result<gst::BufferPool, String> {
    let info = gst_video::VideoInfo::from_caps(caps).map_err(|e| e.to_string())?;
    // SAFETY: a live GstCudaContext from a GstCudaMemory; the call returns
    // a new reference or NULL.
    let raw = unsafe { (api.pool_new)(context) };
    if raw.is_null() {
        return Err("no CUDA buffer pool".into());
    }
    // SAFETY: a new, owned reference.
    let pool: gst::BufferPool = unsafe { from_glib_full(raw) };
    let mut config = pool.config();
    config.set_params(Some(caps), info.size() as u32, 0, 0);
    config.add_option(gst_video::BUFFER_POOL_OPTION_VIDEO_META.as_ref());
    pool.set_config(config).map_err(|e| format!("the CUDA pool refused its settings: {e}"))?;
    pool.set_active(true).map_err(|e| format!("the CUDA pool would not start: {e}"))?;
    Ok(pool)
}

/// Map, launch for the luma and the interleaved chroma, wait, unmap.
unsafe fn run(api: &Api, context: *mut c_void, cur: *mut CudaMemory, prev: Option<*mut CudaMemory>, out: *mut CudaMemory, width: u32, height: u32, keep: i32) -> Result<(), String> {
    let mut mc: gst::ffi::GstMapInfo = std::mem::zeroed();
    let mut mp: gst::ffi::GstMapInfo = std::mem::zeroed();
    let mut mo: gst::ffi::GstMapInfo = std::mem::zeroed();
    let read = gst::ffi::GST_MAP_READ | MAP_CUDA;
    let write = gst::ffi::GST_MAP_WRITE | MAP_CUDA;
    if gst::ffi::gst_memory_map(cur as *mut _, &mut mc, read) == 0 {
        return Err("could not map the picture on the card".into());
    }
    let prev = prev.filter(|p| gst::ffi::gst_memory_map(*p as *mut _, &mut mp, read) != 0);
    if gst::ffi::gst_memory_map(out as *mut _, &mut mo, write) == 0 {
        gst::ffi::gst_memory_unmap(cur as *mut _, &mut mc);
        if let Some(p) = prev {
            gst::ffi::gst_memory_unmap(p as *mut _, &mut mp);
        }
        return Err("could not map the pool's picture on the card".into());
    }
    let result = launch_all(api, context, cur, prev.map(|p| (p, mp.data as u64)), out, mc.data as u64, mo.data as u64, width, height, keep);
    gst::ffi::gst_memory_unmap(out as *mut _, &mut mo);
    if let Some(p) = prev {
        gst::ffi::gst_memory_unmap(p as *mut _, &mut mp);
    }
    gst::ffi::gst_memory_unmap(cur as *mut _, &mut mc);
    result
}

#[allow(clippy::too_many_arguments)]
unsafe fn launch_all(
    api: &Api,
    context: *mut c_void,
    cur: *mut CudaMemory,
    prev: Option<(*mut CudaMemory, u64)>,
    out: *mut CudaMemory,
    cur_ptr: u64,
    out_ptr: u64,
    width: u32,
    height: u32,
    keep: i32,
) -> Result<(), String> {
    if (api.push)(context) == 0 {
        return Err("could not make the card's context current".into());
    }
    let result = (|| {
        let f = function(api, context)?;
        let stream = (api.get_stream)(cur as *mut c_void);
        let stream = if stream.is_null() { std::ptr::null_mut() } else { (api.stream_handle)(stream) };
        // Luma: one byte a sample, the full height. Chroma (NV12's second
        // plane): U and V interleaved, two bytes a sample, half the height.
        let planes = [(0usize, width as i32, height as i32, 1i32), (1usize, (width.div_ceil(2) * 2) as i32, height.div_ceil(2) as i32, 2i32)];
        for (plane, w, h, step) in planes {
            let ci = &(*cur).info;
            let oi = &(*out).info;
            let mut a_cur = cur_ptr + ci.offset[plane] as u64;
            let mut a_out = out_ptr + oi.offset[plane] as u64;
            let (mut a_prev, mut prev_pitch, mut has_prev) = (a_cur, ci.stride[plane], 0i32);
            if let Some((p, ptr)) = prev {
                a_prev = ptr + (*p).info.offset[plane] as u64;
                prev_pitch = (*p).info.stride[plane];
                has_prev = 1;
            }
            let (mut w, mut h, mut cp, mut op, mut keep, mut step) = (w, h, ci.stride[plane], oi.stride[plane], keep, step);
            let mut params: [*mut c_void; 11] = [
                &mut a_cur as *mut _ as *mut c_void,
                &mut a_prev as *mut _ as *mut c_void,
                &mut a_out as *mut _ as *mut c_void,
                &mut w as *mut _ as *mut c_void,
                &mut h as *mut _ as *mut c_void,
                &mut cp as *mut _ as *mut c_void,
                &mut prev_pitch as *mut _ as *mut c_void,
                &mut op as *mut _ as *mut c_void,
                &mut keep as *mut _ as *mut c_void,
                &mut has_prev as *mut _ as *mut c_void,
                &mut step as *mut _ as *mut c_void,
            ];
            let (bx, by) = (32u32, 8u32);
            let r = (api.launch)(f, (w as u32).div_ceil(bx), (h as u32).div_ceil(by), 1, bx, by, 1, 0, stream, params.as_mut_ptr(), std::ptr::null_mut());
            if r != 0 {
                return Err(format!("the deinterlace kernel would not start ({r})"));
            }
        }
        let r = (api.sync)(stream);
        if r != 0 {
            return Err(format!("the deinterlace kernel failed ({r})"));
        }
        Ok(())
    })();
    let mut popped = std::ptr::null_mut();
    (api.pop)(&mut popped);
    result
}
