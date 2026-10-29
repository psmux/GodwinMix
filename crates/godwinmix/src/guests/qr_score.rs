//! Choosing a QR mask: the eight masks, and the score that picks the
//! cleanest. Any mask decodes; this only picks one a camera reads easily.

use super::qr_grid::Grid;

impl Grid {
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

pub fn masked(m: usize, x: usize, y: usize) -> bool {
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

