//! The few CUDA calls the card's deinterlacer makes, opened at run time.
//!
//! As in `cuda_ctx`, nothing is linked: a machine with no NVIDIA driver or
//! no GStreamer CUDA library gets `None` and deinterlaces on the CPU as it
//! did before. The PTX is compiled by the driver for whichever card loads
//! it, once per CUDA context, and kept for the life of the process.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use libloading::Library;

pub type Ptr = *mut c_void;

/// The kernel, built from `deinterlace.cu` by `dev/build-deinterlace-ptx.sh`.
const PTX: &str = concat!(include_str!("deinterlace.ptx"), "\0");

macro_rules! api {
    ($cuda:ident, $gstcuda:ident; $($field:ident: $lib:ident $name:literal fn($($arg:ty),*) -> $ret:ty;)*) => {
        pub struct Api { $(pub $field: unsafe extern "C" fn($($arg),*) -> $ret,)* }
        // SAFETY: each symbol is looked up by its C name and given the type of
        // its C declaration; both libraries are leaked, so the pointers stay good.
        unsafe fn open() -> Result<Api, String> {
            let $cuda = Library::new("libcuda.so.1").map_err(|e| format!("no CUDA driver: {e}"))?;
            let $gstcuda = Library::new("libgstcuda-1.0.so.0").map_err(|e| format!("no GStreamer CUDA library: {e}"))?;
            let api = Api { $($field: *$lib.get(concat!($name, "\0").as_bytes()).map_err(|e| e.to_string())?,)* };
            std::mem::forget($cuda);
            std::mem::forget($gstcuda);
            Ok(api)
        }
    };
}

api! {
    cuda, gstcuda;
    module_load: cuda "cuModuleLoadData" fn(*mut Ptr, *const c_void) -> i32;
    function: cuda "cuModuleGetFunction" fn(*mut Ptr, Ptr, *const u8) -> i32;
    launch: cuda "cuLaunchKernel" fn(Ptr, u32, u32, u32, u32, u32, u32, u32, Ptr, *mut Ptr, *mut Ptr) -> i32;
    sync: cuda "cuStreamSynchronize" fn(Ptr) -> i32;
    alloc: cuda "cuMemAlloc_v2" fn(*mut u64, usize) -> i32;
    free: cuda "cuMemFree_v2" fn(u64) -> i32;
    copy: cuda "cuMemcpyDtoDAsync_v2" fn(u64, u64, usize, Ptr) -> i32;
    current: cuda "cuCtxGetCurrent" fn(*mut Ptr) -> i32;
    context_id: cuda "cuCtxGetId" fn(Ptr, *mut u64) -> i32;
    push: gstcuda "gst_cuda_context_push" fn(Ptr) -> i32;
    pop: gstcuda "gst_cuda_context_pop" fn(*mut Ptr) -> i32;
    is_cuda: gstcuda "gst_is_cuda_memory" fn(*mut gstreamer::ffi::GstMemory) -> i32;
    stream: gstcuda "gst_cuda_memory_get_stream" fn(*mut gstreamer::ffi::GstMemory) -> Ptr;
    stream_handle: gstcuda "gst_cuda_stream_get_handle" fn(Ptr) -> Ptr;
}

/// The CUDA calls, or why there are none. Opened once.
pub fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    // SAFETY: see `open`.
    API.get_or_init(|| unsafe { open() }).as_ref().map_err(Clone::clone)
}

/// The kernel in the context current on this thread, loaded the first time
/// that context asks for it.
///
/// # Safety
/// A CUDA context must be current on the calling thread.
pub unsafe fn kernel(api: &Api) -> Result<Ptr, String> {
    // By the context's id, not its address: a context made after another
    // was destroyed can get the same address, and the old kernel with it.
    static LOADED: Mutex<Option<HashMap<u64, usize>>> = Mutex::new(None);
    let (mut ctx, mut id) = (std::ptr::null_mut(), 0u64);
    if (api.current)(&mut ctx) != 0 || ctx.is_null() || (api.context_id)(ctx, &mut id) != 0 {
        return Err("no CUDA context is current".into());
    }
    let mut loaded = LOADED.lock().unwrap_or_else(|p| p.into_inner());
    let map = loaded.get_or_insert_with(HashMap::new);
    if let Some(f) = map.get(&id) {
        return Ok(*f as Ptr);
    }
    let (mut module, mut f) = (std::ptr::null_mut(), std::ptr::null_mut());
    let code = (api.module_load)(&mut module, PTX.as_ptr().cast());
    if code != 0 {
        return Err(format!("the driver would not load the deinterlace kernel (CUDA error {code})"));
    }
    if (api.function)(&mut f, module, b"gmx_deinterlace\0".as_ptr()) != 0 {
        return Err("the deinterlace kernel has no gmx_deinterlace".into());
    }
    map.insert(id, f as usize);
    Ok(f)
}
