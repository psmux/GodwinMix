//! The module grid of a QR code: the fixed patterns, the zigzag the data is
//! laid in, the eight masks and the score that picks one. `qr.rs` does the
//! bytes. A port of `ui/panels/channels/qr-grid.js`, line for line.

#[derive(Clone)]
pub struct Grid {
    size: usize,
    pub dark: Vec<Vec<bool>>,
    fixed: Vec<Vec<bool>>,
}

impl Grid {
    pub fn new(size: usize) -> Grid {
        Grid { size, dark: vec![vec![false; size]; size], fixed: vec![vec![false; size]; size] }
    }

    fn set(&mut self, x: usize, y: usize, dark: bool) {
        self.dark[y][x] = dark;
        self.fixed[y][x] = true;
    }

    /// Finders, timing, alignment, the version blocks and a reserved format area.
    pub fn functions(&mut self, ver: usize) {
        let n = self.size;
        for i in 0..n {
            self.set(6, i, i % 2 == 0);
            self.set(i, 6, i % 2 == 0);
        }
        for (x, y) in [(3, 3), (n - 4, 3), (3, n - 4)] {
            self.square(x, y, 4, |d| d != 2 && d != 4);
        }
        let at = alignments(ver, n);
        let last = at.len().saturating_sub(1);
        for (i, x) in at.iter().enumerate() {
            for (j, y) in at.iter().enumerate() {
                let corner = (i == 0 && j == 0) || (i == 0 && j == last) || (i == last && j == 0);
                if !corner {
                    self.square(*x, *y, 2, |d| d != 1);
                }
            }
        }
        self.format(0);
        if ver >= 7 {
            self.version(ver);
        }
    }

    fn square(&mut self, cx: usize, cy: usize, r: i64, dark_at: impl Fn(i64) -> bool) {
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (cx as i64 + dx, cy as i64 + dy);
                if x >= 0 && (x as usize) < self.size && y >= 0 && (y as usize) < self.size {
                    self.set(x as usize, y as usize, dark_at(dx.abs().max(dy.abs())));
                }
            }
        }
    }

    pub fn format(&mut self, mask: usize) {
        let data = mask as u32; // level M is 00 in the two high bits
        let mut rem = data;
        for _ in 0..10 {
            rem = (rem << 1) ^ ((rem >> 9) * 0x537);
        }
        let bits = ((data << 10) | rem) ^ 0x5412;
        let bit = |i: usize| (bits >> i) & 1 != 0;
        let n = self.size;
        for i in 0..=5 {
            self.set(8, i, bit(i));
        }
        self.set(8, 7, bit(6));
        self.set(8, 8, bit(7));
        self.set(7, 8, bit(8));
        for i in 9..15 {
            self.set(14 - i, 8, bit(i));
        }
        for i in 0..8 {
            self.set(n - 1 - i, 8, bit(i));
        }
        for i in 8..15 {
            self.set(8, n - 15 + i, bit(i));
        }
        self.set(8, n - 8, true);
    }

    fn version(&mut self, ver: usize) {
        let mut rem = ver as u32;
        for _ in 0..12 {
            rem = (rem << 1) ^ ((rem >> 11) * 0x1f25);
        }
        let bits = ((ver as u32) << 12) | rem;
        for i in 0..18 {
            let dark = (bits >> i) & 1 != 0;
            let (a, b) = (self.size - 11 + i % 3, i / 3);
            self.set(a, b, dark);
            self.set(b, a, dark);
        }
    }

    /// The data, two columns at a time, up and down from the bottom right.
    pub fn codewords(&mut self, data: &[u8]) {
        let n = self.size;
        let mut i = 0;
        let mut right = n as i64 - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vert in 0..n {
                for j in 0..2 {
                    let x = (right - j) as usize;
                    let up = (right + 1) & 2 == 0;
                    let y = if up { n - 1 - vert } else { vert };
                    if self.fixed[y][x] || i >= data.len() * 8 {
                        continue;
                    }
                    self.dark[y][x] = (data[i >> 3] >> (7 - (i & 7))) & 1 != 0;
                    i += 1;
                }
            }
            right -= 2;
        }
    }

    pub fn mask(&mut self, m: usize) {
        for y in 0..self.size {
            for x in 0..self.size {
                if !self.fixed[y][x] && masked(m, x, y) {
                    self.dark[y][x] = !self.dark[y][x];
                }
            }
        }
    }

    /// Runs, blocks and balance. Any mask decodes; this only picks a cleaner one.
    pub fn penalty(&self) -> usize {
        let (n, d) = (self.size, &self.dark);
        let mut score = 0;
        for a in 0..n {
            let (mut row_run, mut col_run) = (1, 1);
            for b in 1..n {
                row_run = if d[a][b] == d[a][b - 1] { row_run + 1 } else { 1 };
                col_run = if d[b][a] == d[b - 1][a] { col_run + 1 } else { 1 };
                for run in [row_run, col_run] {
                    score += match run {
                        5 => 3,
                        r if r > 5 => 1,
                        _ => 0,
                    };
                }
            }
        }
        let mut dark = 0usize;
        for y in 0..n {
            for x in 0..n {
                dark += usize::from(d[y][x]);
                if x < n - 1 && y < n - 1 && d[y][x] == d[y][x + 1] && d[y][x] == d[y + 1][x] && d[y][x] == d[y + 1][x + 1] {
                    score += 3;
                }
            }
        }
        score + ((dark * 20).abs_diff(n * n * 10) / (n * n)) * 10
    }
}

fn masked(m: usize, x: usize, y: usize) -> bool {
    match m {
        0 => (x + y) % 2 == 0,
        1 => y % 2 == 0,
        2 => x % 3 == 0,
        3 => (x + y) % 3 == 0,
        4 => (x / 3 + y / 2) % 2 == 0,
        5 => (x * y) % 2 + (x * y) % 3 == 0,
        6 => ((x * y) % 2 + (x * y) % 3) % 2 == 0,
        _ => ((x + y) % 2 + (x * y) % 3) % 2 == 0,
    }
}

fn alignments(ver: usize, size: usize) -> Vec<usize> {
    if ver == 1 {
        return Vec::new();
    }
    let count = ver / 7 + 2;
    let step = (ver * 4 + 4).div_ceil(count * 2 - 2) * 2;
    let mut out = vec![6];
    let mut pos = size - 7;
    while out.len() < count {
        out.insert(1, pos);
        pos -= step;
    }
    out
}
