//! Softening the matte's edge inwards.
//!
//! A box blur of the block alphas, two passes, then each block keeps the
//! smaller of its own alpha and the blurred one. Taking the smaller means the
//! edge only ever gets softer on the inside: a clear block never gains
//! alpha, so the blur cannot pull a halo of screen colour out past the
//! presenter. It also chokes the edge by about the radius, which is what
//! takes away the last line of green a hard key leaves round hair.

/// Soften `alpha`, `w` by `h` blocks, over `radius` blocks.
pub fn soften(alpha: &mut [u8], w: usize, h: usize, radius: usize) {
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    let mut sums = vec![0u16; w * h];
    let span = (2 * radius + 1) as u32;
    // Across each row into `sums`, then down each column back into a blurred
    // value, which is compared with the original in place.
    for y in 0..h {
        let row = &alpha[y * w..(y + 1) * w];
        let out = &mut sums[y * w..(y + 1) * w];
        box_line(row.iter().map(|a| *a as u32), out, span, radius);
    }
    let mut column = vec![0u16; h];
    let mut blurred = vec![0u16; h];
    for x in 0..w {
        for y in 0..h {
            column[y] = sums[y * w + x];
        }
        box_line(column.iter().map(|a| *a as u32), &mut blurred, span, radius);
        for y in 0..h {
            let a = &mut alpha[y * w + x];
            *a = (*a).min(blurred[y].min(255) as u8);
        }
    }
}

/// A running box sum along one line, the edges repeating the end values.
fn box_line(line: impl Iterator<Item = u32> + Clone, out: &mut [u16], span: u32, radius: usize) {
    let values: Vec<u32> = line.collect();
    let n = values.len();
    let at = |i: isize| values[i.clamp(0, n as isize - 1) as usize];
    let mut sum: u32 = (-(radius as isize)..=radius as isize).map(at).sum();
    for (i, o) in out.iter_mut().enumerate().take(n) {
        *o = ((sum + span / 2) / span) as u16;
        sum += at(i as isize + radius as isize + 1);
        sum -= at(i as isize - radius as isize);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hard_edge_goes_soft_on_the_inside_only() {
        let (w, h) = (8, 1);
        let mut alpha = vec![0, 0, 0, 0, 255, 255, 255, 255];
        soften(&mut alpha, w, h, 1);
        assert_eq!(&alpha[..4], &[0, 0, 0, 0], "nothing outside gained alpha");
        assert!(alpha[4] > 0 && alpha[4] < 255, "the first solid block is now partly clear: {alpha:?}");
        assert_eq!(alpha[7], 255, "the middle is untouched");
    }
}
