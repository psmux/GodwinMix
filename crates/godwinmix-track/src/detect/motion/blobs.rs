//! Joining changed blocks into boxes: two pass connected component labelling
//! with union find, on the block grid dilated by one block so that a body
//! split by a still arm or a dark shirt stays one box.

/// One connected group of changed blocks, in block coordinates, inclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Blob {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
    /// How many changed blocks it holds.
    pub blocks: usize,
}

/// The block grid and its scratch space, kept between frames.
#[derive(Debug, Default)]
pub struct Blobs {
    bw: usize,
    bh: usize,
    counts: Vec<u16>,
    labels: Vec<u32>,
    parent: Vec<u32>,
    stats: Vec<Blob>,
    out: Vec<Blob>,
}

impl Blobs {
    /// Clears the counts for a `bw` by `bh` grid.
    pub fn start(&mut self, bw: usize, bh: usize) {
        (self.bw, self.bh) = (bw, bh);
        self.counts.clear();
        self.counts.resize(bw * bh, 0);
        // At most one new label per two cells, so these never grow later.
        let most = bw * bh / 2 + 2;
        self.parent.reserve(most.saturating_sub(self.parent.len()));
        self.stats.reserve(most.saturating_sub(self.stats.len()));
        self.out.reserve(most.saturating_sub(self.out.len()));
    }

    /// One more changed pixel in block `(bx, by)`.
    #[inline]
    pub fn count(&mut self, bx: usize, by: usize) {
        if bx < self.bw && by < self.bh {
            let c = &mut self.counts[by * self.bw + bx];
            *c = c.saturating_add(1);
        }
    }

    fn active(&self, x: usize, y: usize, min: u16) -> bool {
        self.counts[y * self.bw + x] >= min
    }

    fn dilated(&self, x: usize, y: usize, min: u16) -> bool {
        let (x0, x1) = (x.saturating_sub(1), (x + 1).min(self.bw - 1));
        let (y0, y1) = (y.saturating_sub(1), (y + 1).min(self.bh - 1));
        (y0..=y1).any(|yy| (x0..=x1).any(|xx| self.active(xx, yy, min)))
    }

    fn root(&mut self, mut a: u32) -> u32 {
        while self.parent[a as usize] != a {
            let up = self.parent[self.parent[a as usize] as usize];
            self.parent[a as usize] = up;
            a = up;
        }
        a
    }

    fn union(&mut self, a: u32, b: u32) -> u32 {
        let (ra, rb) = (self.root(a), self.root(b));
        let (lo, hi) = (ra.min(rb), ra.max(rb));
        self.parent[hi as usize] = lo;
        lo
    }

    /// Labels blocks with at least `min` changed pixels and gathers a box per
    /// group, read back with [`boxes`](Blobs::boxes).
    pub fn label(&mut self, min: u16) {
        let (bw, bh) = (self.bw, self.bh);
        self.labels.clear();
        self.labels.resize(bw * bh, 0);
        self.parent.clear();
        self.parent.push(0);
        for y in 0..bh {
            for x in 0..bw {
                if self.dilated(x, y, min) {
                    self.labels[y * bw + x] = self.join_neighbours(x, y);
                }
            }
        }
        self.gather(min);
    }

    /// The label for `(x, y)` from its already labelled neighbours (west,
    /// north west, north, north east), or a new one.
    fn join_neighbours(&mut self, x: usize, y: usize) -> u32 {
        let bw = self.bw;
        let mut label = 0;
        let near = [(x > 0, 0, 1), (x > 0 && y > 0, bw + 1, 0), (y > 0, bw, 0), (y > 0 && x + 1 < bw, bw - 1, 0)];
        for (ok, back, west) in near {
            if !ok {
                continue;
            }
            let i = y * bw + x - back - west;
            let n = self.labels[i];
            if n != 0 {
                label = if label == 0 { self.root(n) } else { self.union(label, n) };
            }
        }
        if label == 0 {
            label = self.parent.len() as u32;
            self.parent.push(label);
        }
        label
    }

    fn gather(&mut self, min: u16) {
        self.stats.clear();
        let empty = Blob { x0: usize::MAX, y0: usize::MAX, x1: 0, y1: 0, blocks: 0 };
        self.stats.resize(self.parent.len(), empty);
        for y in 0..self.bh {
            for x in 0..self.bw {
                let l = self.labels[y * self.bw + x];
                if l == 0 || !self.active(x, y, min) {
                    continue;
                }
                let r = self.root(l) as usize;
                let s = &mut self.stats[r];
                (s.x0, s.y0) = (s.x0.min(x), s.y0.min(y));
                (s.x1, s.y1) = (s.x1.max(x), s.y1.max(y));
                s.blocks += 1;
            }
        }
        self.out.clear();
        self.out.extend(self.stats.iter().filter(|s| s.blocks > 0));
        while super::merge::merge_once(&mut self.out) {}
    }

    pub fn boxes(&self) -> &[Blob] {
        &self.out
    }
}

#[cfg(test)]
#[path = "blobs_tests.rs"]
mod tests;
