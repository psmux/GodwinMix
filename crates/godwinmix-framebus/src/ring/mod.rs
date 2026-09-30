//! The ring: which slot the owner writes next, and how a reader holds one.
//!
//! The owner never waits. It writes into the lowest numbered slot that is not
//! the newest frame and that no reader leases; if every slot is leased it
//! drops the frame and counts it. A reader always takes the newest frame, so
//! a reader that falls behind skips frames and nobody else notices. Lowest
//! numbered first keeps the pages actually touched to a handful when readers
//! keep up, however many slots the region has room for.
//!
//! The one race is a reader leasing a slot at the moment the owner starts to
//! write it. Both sides use the store then load pattern with `SeqCst`: the
//! owner marks the slot `WRITING` and then reads the leases, the reader sets
//! its lease and then reads the slot's sequence number. In the single total
//! order of `SeqCst` operations one of them comes second and sees the other,
//! and that one backs off.

use crate::format::Layout;
use crate::header::{Header, Slot, MAGIC, MAX_READERS, MAX_SLOTS};
use crate::shm::{page_size, Region};
use crate::Error;

mod owner;
#[cfg(test)]
mod pages;
mod reader;
#[cfg(test)]
mod tests;

pub struct Ring {
    region: Region,
}

/// What a reader got when it asked for the newest frame.
#[derive(Debug, PartialEq, Eq)]
pub enum Lease {
    /// Nothing newer than what it already has.
    Nothing,
    /// It already holds as many frames as the owner allows one reader.
    Full,
    Leased {
        slot: usize,
        seq: u64,
        skipped: u64,
    },
}

/// A frame's timing, written with it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Meta {
    pub pts: u64,
    pub duration: u64,
    pub captured_ns: u64,
    pub checksum: u64,
}

impl Ring {
    /// A new region sized for `layout`, with room for `max_readers` readers
    /// each holding up to `leases` frames at once.
    pub fn create(layout: &Layout, max_readers: usize, leases: usize) -> Result<Ring, Error> {
        let page = page_size();
        let header_len = std::mem::size_of::<Header>().next_multiple_of(page);
        let slot_stride = (layout.size as usize).next_multiple_of(page);
        let n_slots = (max_readers.clamp(1, MAX_READERS) * leases.max(1) + 2).min(MAX_SLOTS);
        let region = Region::create(header_len + n_slots * slot_stride)?;
        let h = region.header_ptr().cast::<Header>();
        // SAFETY: the region is zeroed, page aligned and at least a Header
        // long; zero is a valid value for every atomic in it, and nobody else
        // can see it until its handle is sent.
        unsafe {
            (*h).magic = MAGIC;
            (*h).header_size = header_len as u64;
            (*h).format = layout.format as u32;
            (*h).width = layout.width;
            (*h).height = layout.height;
            (*h).n_planes = layout.n_planes;
            (*h).offsets = layout.offsets;
            (*h).strides = layout.strides;
            (*h).frame_size = layout.size;
            (*h).slot_stride = slot_stride as u64;
            (*h).data_offset = header_len as u64;
            (*h).n_slots = n_slots as u32;
            (*h).leases_per_reader = leases.max(1) as u32;
            (*h).owner_pid = std::process::id();
            (*h).fps_n = layout.fps_n;
            (*h).fps_d = layout.fps_d;
        }
        Ok(Ring { region })
    }

    /// Wrap a region another process made, after checking it is one.
    pub fn attach(region: Region) -> Result<Ring, Error> {
        if region.header_len() < std::mem::size_of::<Header>() {
            return Err(Error::Protocol(
                "the region is too small for a frame bus header".into(),
            ));
        }
        let ring = Ring { region };
        let h = ring.header();
        if h.magic != MAGIC {
            return Err(Error::Protocol(
                "the region is not a frame bus region of this version. \
                 Update the owner and the reader together"
                    .into(),
            ));
        }
        if h.layout().is_none()
            || h.n_slots as usize > MAX_SLOTS
            || h.frame_size > h.slot_stride
            || h.data_offset as usize != ring.region.header_len()
        {
            return Err(Error::Protocol(
                "the region header describes an impossible layout".into(),
            ));
        }
        Ok(ring)
    }

    pub fn header(&self) -> &Header {
        // SAFETY: create and attach both checked the mapping holds a Header.
        unsafe { &*self.region.header_ptr().cast::<Header>() }
    }

    pub fn region(&self) -> &Region {
        &self.region
    }

    pub fn total_len(&self) -> usize {
        let h = self.header();
        (h.data_offset + h.n_slots as u64 * h.slot_stride) as usize
    }

    /// The first byte of slot `slot`'s frame, `frame_size` bytes long.
    pub fn data(&self, slot: usize) -> *mut u8 {
        let h = self.header();
        self.region
            .at((h.data_offset + slot as u64 * h.slot_stride) as usize)
    }

    /// The frame in `slot` as bytes. Only for a slot the caller leases (a
    /// reader) or has claimed (the owner).
    pub fn frame(&self, slot: usize) -> &[u8] {
        // SAFETY: data() points at frame_size mapped bytes; the lease or the
        // claim is what keeps the other side from writing them.
        unsafe { std::slice::from_raw_parts(self.data(slot), self.header().frame_size as usize) }
    }

    pub fn slot(&self, slot: usize) -> &Slot {
        &self.header().slots[slot]
    }
}
