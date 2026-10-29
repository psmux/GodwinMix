//! Where a move lands: markers counted from the place the prompter is, and
//! the same place found again in rows made at another width.

use super::Inner;

/// `step` markers on from where the prompter is. Back from a little way into
/// a section is the start of that section, which is what a pedal tap means.
pub(super) fn stepped(inner: &Inner, step: i32) -> f64 {
    let here = inner.position;
    let starts: Vec<f64> = inner.markers.iter().map(|m| m.row as f64).collect();
    if step > 0 {
        let ahead: Vec<f64> = starts.iter().copied().filter(|r| *r > here + 0.01).collect();
        return ahead.get(step as usize - 1).copied().unwrap_or(inner.rows.len() as f64);
    }
    let behind: Vec<f64> = starts.iter().copied().filter(|r| *r < here - 0.5).collect();
    let back = (-step) as usize;
    if back > behind.len() { 0.0 } else { behind[behind.len() - back] }
}

impl Inner {
    /// The line as written that the position is in, and how far through it.
    pub(super) fn place(&self) -> (u32, f64) {
        let at = (self.position.floor() as usize).min(self.rows.len().saturating_sub(1));
        let Some(row) = self.rows.get(at) else { return (0, 0.0) };
        let first = self.rows.iter().position(|r| r.line == row.line).unwrap_or(at);
        let count = self.rows.iter().filter(|r| r.line == row.line).count().max(1);
        (row.line, ((self.position - first as f64) / count as f64).clamp(0.0, 1.0))
    }

    /// The same place in rows made at another width.
    pub(super) fn row_of(&self, (line, through): (u32, f64)) -> f64 {
        let Some(first) = self.rows.iter().position(|r| r.line == line) else { return 0.0 };
        let count = self.rows.iter().filter(|r| r.line == line).count();
        first as f64 + through * count as f64
    }
}
