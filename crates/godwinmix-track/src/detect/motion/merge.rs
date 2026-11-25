//! Joining boxes that are two halves of one person.

use super::blobs::Blob;

/// Joins two boxes side by side at about the same height with a gap
/// under a quarter of their height: the leading and trailing edges of
/// one person whose plain clothes show no change in the middle.
pub fn merge_once(out: &mut Vec<Blob>) -> bool {
    for i in 0..out.len() {
        for j in i + 1..out.len() {
            let (a, b) = (out[i], out[j]);
            let overlap = a.y1.min(b.y1) as isize - a.y0.max(b.y0) as isize + 1;
            let shorter = (a.y1 - a.y0).min(b.y1 - b.y0) as isize + 1;
            let gap = a.x0.max(b.x0) as isize - a.x1.min(b.x1) as isize - 1;
            let taller = (a.y1 - a.y0).max(b.y1 - b.y0) as isize + 1;
            if overlap * 2 >= shorter && gap * 4 <= taller {
                out[i] = Blob {
                    x0: a.x0.min(b.x0),
                    y0: a.y0.min(b.y0),
                    x1: a.x1.max(b.x1),
                    y1: a.y1.max(b.y1),
                    blocks: a.blocks + b.blocks,
                };
                out.swap_remove(j);
                return true;
            }
        }
    }
    false
}
