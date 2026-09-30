//! The bytes at the start of a shared memory region, as both sides see them.
//!
//! Everything a reader and the owner both write is an atomic. Everything the
//! owner writes once, before any reader can see the region, is plain data.
//! The layout is `repr(C)` and versioned by [`MAGIC`]; a reader that finds a
//! different magic refuses the region rather than guess.

use std::sync::atomic::{AtomicU32, AtomicU64};

use crate::format::{Format, Layout};

/// "GMXBUS" and the layout version. Bump the last byte when a field moves.
pub const MAGIC: u64 = u64::from_le_bytes(*b"GMXBUS02");
/// A lease is one bit in a `u64`, so a region has at most 64 slots.
pub const MAX_SLOTS: usize = 64;
/// Readers attached to one region at once.
pub const MAX_READERS: usize = 32;
/// The value of `Slot::seq` while the owner writes into it.
pub const WRITING: u64 = u64::MAX;
/// `Slot::pts` when the frame had no timestamp.
pub const NO_PTS: u64 = u64::MAX;

/// One slot's description of the frame in it. Aligned to a cache line so the
/// owner writing one slot never bounces a line a reader of another is reading.
#[repr(C, align(64))]
#[derive(Default)]
pub struct Slot {
    /// The frame's sequence number, [`WRITING`] while it is being written,
    /// 0 before it ever held one. Stored last with `Release`.
    pub seq: AtomicU64,
    pub pts: AtomicU64,
    pub duration: AtomicU64,
    /// Monotonic nanoseconds when the owner was handed the frame.
    pub captured_ns: AtomicU64,
    /// Monotonic nanoseconds when the frame became readable.
    pub published_ns: AtomicU64,
    /// FNV-1a over the frame's bytes, or 0 when the owner does not checksum.
    pub checksum: AtomicU64,
    /// Bytes of the slot the frame fills. A picture fills it; a chunk of
    /// sound may not.
    pub len: AtomicU64,
}

/// One reader's place in the region. The owner hands a reader its index when
/// it connects and clears the place when its socket closes, which is how a
/// reader killed mid read leaves nothing locked.
#[repr(C, align(64))]
#[derive(Default)]
pub struct Reader {
    /// 1 while a reader holds this place.
    pub live: AtomicU32,
    pub pid: AtomicU32,
    /// Bit `i` set: this reader is reading slot `i`, and the owner will not
    /// write into it.
    pub leases: AtomicU64,
    /// Frames this reader took, and frames it never saw because it was late.
    pub delivered: AtomicU64,
    pub skipped: AtomicU64,
}

#[repr(C)]
pub struct Header {
    pub magic: u64,
    pub header_size: u64,
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub n_planes: u32,
    pub offsets: [u64; 4],
    pub strides: [u32; 4],
    pub frame_size: u64,
    /// Bytes from one slot's data to the next, a whole number of pages.
    pub slot_stride: u64,
    /// Where slot 0's data starts, a whole number of pages into the region.
    pub data_offset: u64,
    pub n_slots: u32,
    pub leases_per_reader: u32,
    pub owner_pid: u32,
    /// Frame rate as the owner negotiated it; 0/1 when it varies.
    pub fps_n: u32,
    pub fps_d: u32,
    pub _pad: u32,
    /// `seq << 8 | slot` of the newest frame, 0 before the first.
    pub latest: AtomicU64,
    pub published: AtomicU64,
    /// Frames the owner could not place because every slot was leased.
    pub dropped: AtomicU64,
    /// 1 once the owner replaced this region with another (a new format) or
    /// shut down. A reader holding frames from it may keep reading them.
    pub retired: AtomicU32,
    pub _pad2: u32,
    pub slots: [Slot; MAX_SLOTS],
    pub readers: [Reader; MAX_READERS],
}

impl Header {
    pub fn layout(&self) -> Option<Layout> {
        Some(Layout {
            format: Format::from_code(self.format)?,
            width: self.width,
            height: self.height,
            n_planes: self.n_planes,
            offsets: self.offsets,
            strides: self.strides,
            size: self.frame_size,
            fps_n: self.fps_n,
            fps_d: self.fps_d.max(1),
        })
    }
}

/// Pack and unpack `Header::latest`.
pub fn pack_latest(seq: u64, slot: usize) -> u64 {
    (seq << 8) | slot as u64
}

pub fn unpack_latest(v: u64) -> (u64, usize) {
    (v >> 8, (v & 0xff) as usize)
}

/// FNV-1a over eight bytes at a time. Cheap enough for tests and the
/// benchmark's check pass; the owner computes it only when asked.
pub fn checksum(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut chunks = bytes.chunks_exact(8);
    for c in &mut chunks {
        h ^= u64::from_le_bytes(c.try_into().unwrap());
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    for &b in chunks.remainder() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h | 1
}
