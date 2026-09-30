//! What the owner can see of its readers, and the row copy for raw planes.

use std::sync::atomic::Ordering::Relaxed;

use crate::ring::Ring;
use crate::Layout;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReaderStats {
    pub pid: u32,
    pub delivered: u64,
    pub skipped: u64,
    /// Frames it holds right now.
    pub holding: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PublisherStats {
    pub published: u64,
    /// Frames the owner dropped because every slot was leased. Zero unless
    /// readers hold far more frames than they are allowed.
    pub dropped: u64,
    pub slots: u32,
    pub readers: Vec<ReaderStats>,
}

impl PublisherStats {
    pub fn read(ring: &Ring) -> PublisherStats {
        let h = ring.header();
        PublisherStats {
            published: h.published.load(Relaxed),
            dropped: h.dropped.load(Relaxed),
            slots: h.n_slots,
            readers: h
                .readers
                .iter()
                .filter(|r| r.live.load(Relaxed) == 1)
                .map(|r| ReaderStats {
                    pid: r.pid.load(Relaxed),
                    delivered: r.delivered.load(Relaxed),
                    skipped: r.skipped.load(Relaxed),
                    holding: r.leases.load(Relaxed).count_ones(),
                })
                .collect(),
        }
    }
}

/// Copy planes given with their own strides into a slot laid out as
/// `layout`. A plane whose stride already matches is one copy.
pub fn copy_planes(layout: &Layout, planes: &[(&[u8], usize)], dst: &mut [u8]) {
    for (i, &(src, src_stride)) in planes.iter().enumerate().take(layout.n_planes as usize) {
        let off = layout.offsets[i] as usize;
        let stride = layout.strides[i] as usize;
        let rows = layout.rows(i) as usize;
        let row = layout.row_bytes(i);
        if src_stride == stride && src.len() >= rows * stride {
            dst[off..off + rows * stride].copy_from_slice(&src[..rows * stride]);
            continue;
        }
        for y in 0..rows {
            let s = &src[y * src_stride..y * src_stride + row];
            dst[off + y * stride..off + y * stride + row].copy_from_slice(s);
        }
    }
}
