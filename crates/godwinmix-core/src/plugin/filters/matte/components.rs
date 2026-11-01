//! Remove small disconnected flecks and enclosed holes at model resolution.
//! Large detached regions survive, including a raised hand beside the body.
pub fn clean(mask: &mut [u8], w: usize, h: usize) {
    if w == 0 || h == 0 { return; }
    let mut seen = vec![false; mask.len()];
    let mut region = Vec::new();
    let limit = (w * h / 1500).max(4);
    for start in 0..w * h {
        if seen[start] { continue; }
        let foreground = mask[start] >= 128;
        region.clear();
        region.push(start);
        seen[start] = true;
        let mut cursor = 0;
        let mut border = false;
        while cursor < region.len() {
            let n = region[cursor];
            let (x, y) = (n % w, n / w);
            border |= x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize { continue; }
                let next = ny as usize * w + nx as usize;
                if !seen[next] && (mask[next] >= 128) == foreground {
                    seen[next] = true;
                    region.push(next);
                }
            }
            cursor += 1;
        }
        if region.len() <= limit && (foreground || !border) {
            for &n in &region { mask[n] = if foreground { 0 } else { 255 }; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_fleck_fills_hole_preserves_hands_and_open_background() {
        let mut m = vec![0; 40 * 40];
        for y in 10..40 { for x in 10..30 { m[y * 40 + x] = 255; } }
        for y in 4..10 { m[y * 40 + 12] = 255; }
        for y in 4..8 { for x in 32..36 { m[y * 40 + x] = 255; } }
        m[20 * 40 + 20] = 0;
        m[2 * 40 + 2] = 255;
        clean(&mut m, 40, 40);
        assert_eq!(m[82], 0);
        assert_eq!(m[820], 255);
        assert_eq!(m[4 * 40 + 12], 255);
        assert_eq!(m[4 * 40 + 32], 255);
        assert_eq!(m[0], 0);
    }
}
