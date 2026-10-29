//! The copy of a plane the card's deinterlacer reads (`cuda_deint`).

use super::cuda_api::{self, Api, Ptr};

/// The copy of a plane the kernel reads, on the card, grown as needed and
/// freed with the stage.
#[derive(Default)]
pub struct Scratch {
    ptr: u64,
    size: usize,
    ctx: usize,
}

// SAFETY: the pointers are only used with the state's lock held.
unsafe impl Send for Scratch {}

impl Scratch {
    /// # Safety
    /// `ctx` is current.
    pub unsafe fn at_least(&mut self, api: &Api, ctx: Ptr, size: usize) -> Result<u64, String> {
        if self.ptr != 0 && self.size >= size && self.ctx == ctx as usize {
            return Ok(self.ptr);
        }
        self.release(api);
        super::cuda_launch::check((api.alloc)(&mut self.ptr, size), "allocating the copy")?;
        gstreamer::ffi::gst_object_ref(ctx.cast());
        (self.size, self.ctx) = (size, ctx as usize);
        Ok(self.ptr)
    }

    /// # Safety
    /// Called with no kernel reading the copy.
    unsafe fn release(&mut self, api: &Api) {
        if self.ptr == 0 {
            return;
        }
        let ctx = self.ctx as Ptr;
        if (api.push)(ctx) != 0 {
            (api.free)(self.ptr);
            (api.pop)(std::ptr::null_mut());
        }
        gstreamer::ffi::gst_object_unref(ctx.cast());
        (self.ptr, self.size, self.ctx) = (0, 0, 0);
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Ok(api) = cuda_api::api() {
            // SAFETY: the stage is gone, so no picture is being worked on.
            unsafe { self.release(api) }
        }
    }
}
