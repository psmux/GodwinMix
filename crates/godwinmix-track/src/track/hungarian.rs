//! The Hungarian method for a square cost matrix, with potentials, in
//! O(n cubed). The buffers live in the struct, so solving the same size again
//! allocates nothing.

/// Reusable scratch space for [`Hungarian::solve`].
#[derive(Default, Debug)]
pub struct Hungarian {
    u: Vec<f64>,
    v: Vec<f64>,
    p: Vec<usize>,
    way: Vec<usize>,
    minv: Vec<f64>,
    used: Vec<bool>,
}

impl Hungarian {
    fn reset(&mut self, k: usize) {
        for buf in [&mut self.u, &mut self.v, &mut self.minv] {
            buf.clear();
            buf.resize(k + 1, 0.0);
        }
        for buf in [&mut self.p, &mut self.way] {
            buf.clear();
            buf.resize(k + 1, 0);
        }
        self.used.clear();
        self.used.resize(k + 1, false);
    }

    /// Finds the assignment of rows to columns with the least total cost.
    /// `cost` is `k` by `k`, row major. On return `assign[row]` is its column.
    pub fn solve(&mut self, cost: &[f64], k: usize, assign: &mut [usize]) {
        debug_assert!(cost.len() >= k * k && assign.len() >= k);
        self.reset(k);
        for i in 1..=k {
            self.augment(cost, k, i);
        }
        for j in 1..=k {
            if self.p[j] != 0 {
                assign[self.p[j] - 1] = j - 1;
            }
        }
    }

    fn augment(&mut self, cost: &[f64], k: usize, i: usize) {
        self.p[0] = i;
        let mut j0 = 0;
        self.minv.fill(f64::INFINITY);
        self.used.fill(false);
        loop {
            self.used[j0] = true;
            let i0 = self.p[j0];
            let (mut delta, mut j1) = (f64::INFINITY, 0);
            for j in 1..=k {
                if self.used[j] {
                    continue;
                }
                let cur = cost[(i0 - 1) * k + j - 1] - self.u[i0] - self.v[j];
                if cur < self.minv[j] {
                    self.minv[j] = cur;
                    self.way[j] = j0;
                }
                if self.minv[j] < delta {
                    delta = self.minv[j];
                    j1 = j;
                }
            }
            for j in 0..=k {
                if self.used[j] {
                    self.u[self.p[j]] += delta;
                    self.v[j] -= delta;
                } else {
                    self.minv[j] -= delta;
                }
            }
            j0 = j1;
            if self.p[j0] == 0 {
                break;
            }
        }
        while j0 != 0 {
            let j1 = self.way[j0];
            self.p[j0] = self.p[j1];
            j0 = j1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brute(cost: &[f64], k: usize) -> f64 {
        fn go(cost: &[f64], k: usize, row: usize, used: &mut Vec<bool>) -> f64 {
            if row == k {
                return 0.0;
            }
            let mut best = f64::INFINITY;
            for c in 0..k {
                if !used[c] {
                    used[c] = true;
                    best = best.min(cost[row * k + c] + go(cost, k, row + 1, used));
                    used[c] = false;
                }
            }
            best
        }
        go(cost, k, 0, &mut vec![false; k])
    }

    #[test]
    fn matches_brute_force_on_pseudo_random_matrices() {
        let mut h = Hungarian::default();
        let mut seed = 7u64;
        for k in 1..=6 {
            for _ in 0..20 {
                let cost: Vec<f64> = (0..k * k)
                    .map(|_| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                        (seed >> 40) as f64 / (1u64 << 24) as f64
                    })
                    .collect();
                let mut assign = vec![0; k];
                h.solve(&cost, k, &mut assign);
                let total: f64 = assign.iter().enumerate().map(|(r, &c)| cost[r * k + c]).sum();
                assert!((total - brute(&cost, k)).abs() < 1e-9);
            }
        }
    }
}
