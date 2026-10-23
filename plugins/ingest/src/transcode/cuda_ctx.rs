//! A CUDA context that sleeps while it waits for the GPU.
//!
//! GStreamer makes its contexts with CUDA's default scheduling, which on a
//! machine with more cores than contexts is to spin: a thread waiting for
//! the decoder or the encoder burns its core until the work is done. With
//! fifty channels and NVENC near its limit that was forty cores of nothing
//! but waiting. So the one context this process shares is made here, with
//! `CU_CTX_SCHED_BLOCKING_SYNC`, and handed to GStreamer wrapped as its own.
//!
//! Both libraries are opened at run time: a machine with no NVIDIA driver or
//! no CUDA plugin gets `None`, and GStreamer makes its own context as before.

use gstreamer as gst;
use gstreamer::glib::translate::from_glib_full;
use libloading::{Library, Symbol};

/// `CU_CTX_SCHED_BLOCKING_SYNC`.
const BLOCKING_SYNC: u32 = 0x04;

type Init = unsafe extern "C" fn(u32) -> i32;
type DeviceGet = unsafe extern "C" fn(*mut i32, i32) -> i32;
type CtxCreate = unsafe extern "C" fn(*mut *mut std::ffi::c_void, u32, i32) -> i32;
type CtxPop = unsafe extern "C" fn(*mut *mut std::ffi::c_void) -> i32;
type LoadLibrary = unsafe extern "C" fn() -> i32;
type Wrap = unsafe extern "C" fn(*mut std::ffi::c_void, i32) -> *mut std::ffi::c_void;
type NewContext = unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut gst::ffi::GstContext;

/// A GStreamer CUDA context on device 0 that blocks rather than spins, or
/// why there is none.
pub fn blocking() -> Result<gst::Context, String> {
    // SAFETY: the libraries stay loaded for the life of the process (they
    // are leaked below), every function is called with the arguments its C
    // declaration takes, and each result is checked before it is used.
    unsafe {
        let cuda = Library::new("libcuda.so.1").map_err(|e| format!("no CUDA driver: {e}"))?;
        let gstcuda = Library::new("libgstcuda-1.0.so.0").map_err(|e| format!("no GStreamer CUDA library: {e}"))?;
        let init: Symbol<Init> = cuda.get(b"cuInit\0").map_err(|e| e.to_string())?;
        let device_get: Symbol<DeviceGet> = cuda.get(b"cuDeviceGet\0").map_err(|e| e.to_string())?;
        let create: Symbol<CtxCreate> = cuda.get(b"cuCtxCreate_v2\0").map_err(|e| e.to_string())?;
        let pop: Symbol<CtxPop> = cuda.get(b"cuCtxPopCurrent_v2\0").map_err(|e| e.to_string())?;
        let load: Symbol<LoadLibrary> = gstcuda.get(b"gst_cuda_load_library\0").map_err(|e| e.to_string())?;
        let wrap: Symbol<Wrap> = gstcuda.get(b"gst_cuda_context_new_wrapped\0").map_err(|e| e.to_string())?;
        let new_context: Symbol<NewContext> = gstcuda.get(b"gst_context_new_cuda_context\0").map_err(|e| e.to_string())?;
        if load() == 0 {
            return Err("GStreamer could not load CUDA".into());
        }
        let (mut device, mut ctx) = (0i32, std::ptr::null_mut());
        if init(0) != 0 || device_get(&mut device, 0) != 0 || create(&mut ctx, BLOCKING_SYNC, device) != 0 {
            return Err("CUDA would not make a context on device 0".into());
        }
        let mut popped = std::ptr::null_mut();
        pop(&mut popped);
        let wrapped = wrap(ctx, device);
        if wrapped.is_null() {
            return Err("GStreamer would not take the CUDA context".into());
        }
        let context = new_context(wrapped);
        gst::ffi::gst_object_unref(wrapped as *mut gst::ffi::GstObject);
        if context.is_null() {
            return Err("GStreamer made no context of the CUDA context".into());
        }
        std::mem::forget(cuda);
        std::mem::forget(gstcuda);
        Ok(from_glib_full(context))
    }
}
