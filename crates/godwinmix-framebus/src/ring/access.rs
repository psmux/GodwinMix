//! A slot's bytes and description, for whichever side holds it.

use super::Ring;
use crate::header::Slot;

impl Ring {
    /// The first byte of slot `slot`'s frame, `frame_size` bytes long.
    pub fn data(&self, slot: usize) -> *mut u8 {
        let h = self.header();
        self.region
            .at((h.data_offset + slot as u64 * h.slot_stride) as usize)
    }

    /// The frame in `slot` as bytes. Only for a slot the caller leases (a
    /// reader) or has claimed (the owner).
    pub fn frame(&self, slot: usize) -> &[u8] {
        let size = self.header().frame_size;
        let len = match self.slot(slot).len.load(std::sync::atomic::Ordering::Relaxed) {
            0 => size,
            n => n.min(size),
        };
        // SAFETY: data() points at frame_size mapped bytes and len is at most
        // that; the lease or the claim keeps the other side from writing them.
        unsafe { std::slice::from_raw_parts(self.data(slot), len as usize) }
    }

    /// Whether this region carries sound, which is read in order.
    pub fn in_order(&self) -> bool {
        self.header().layout().is_some_and(|l| l.is_audio())
    }

    pub fn slot(&self, slot: usize) -> &Slot {
        &self.header().slots[slot]
    }
}
