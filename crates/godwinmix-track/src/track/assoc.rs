//! One matching pass: a cost per (track, detection) pair, gated, solved with
//! the Hungarian method. All buffers are kept between frames.

use super::hungarian::Hungarian;

const GATED: f64 = 1.0e6;

/// Scratch space for matching, reused every frame.
#[derive(Default, Debug)]
pub struct Assoc {
    /// Slot indices taking part in this pass.
    pub rows: Vec<usize>,
    /// Detection indices taking part in this pass.
    pub cols: Vec<usize>,
    /// The matches found, as (slot index, detection index).
    pub pairs: Vec<(usize, usize)>,
    cost: Vec<f64>,
    assign: Vec<usize>,
    hungarian: Hungarian,
}

impl Assoc {
    /// Matches `rows` to `cols` by least total cost. `cost_of(slot, det)`
    /// gives `None` for a pair that may not match at all. Fills `pairs`.
    pub fn solve(&mut self, mut cost_of: impl FnMut(usize, usize) -> Option<f64>) {
        self.pairs.clear();
        let (n, m) = (self.rows.len(), self.cols.len());
        if n == 0 || m == 0 {
            return;
        }
        let k = n.max(m);
        self.cost.clear();
        self.cost.resize(k * k, GATED);
        for (r, &slot) in self.rows.iter().enumerate() {
            for (c, &det) in self.cols.iter().enumerate() {
                if let Some(cost) = cost_of(slot, det) {
                    self.cost[r * k + c] = cost;
                }
            }
        }
        self.assign.clear();
        self.assign.resize(k, 0);
        self.hungarian.solve(&self.cost, k, &mut self.assign);
        for (r, &c) in self.assign.iter().enumerate() {
            if r < n && c < m && self.cost[r * k + c] < GATED {
                self.pairs.push((self.rows[r], self.cols[c]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gated_pairs_never_match_even_when_left_over() {
        let mut a = Assoc::default();
        a.rows.extend([10, 11]);
        a.cols.extend([0, 1, 2]);
        // Slot 10 fits detection 1 only; slot 11 fits nothing.
        a.solve(|s, d| (s == 10 && d == 1).then_some(0.2));
        assert_eq!(a.pairs, vec![(10, 1)]);
    }

    #[test]
    fn picks_the_global_best_not_the_greedy_one() {
        let mut a = Assoc::default();
        a.rows.extend([0, 1]);
        a.cols.extend([0, 1]);
        // Greedy would give slot 0 detection 0 (0.1) and leave slot 1 gated.
        let cost = [[0.1, 0.3], [0.2, f64::NAN]];
        a.solve(|s, d| Some(cost[s][d]).filter(|c| !c.is_nan()));
        a.pairs.sort();
        assert_eq!(a.pairs, vec![(0, 1), (1, 0)]);
    }
}
