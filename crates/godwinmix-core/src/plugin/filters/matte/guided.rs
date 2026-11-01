//! Fast guided upsampling, following He et al. Coefficients are fitted on a
//! subsampled luma guide and evaluated at the compositor's 2x2 block centres.
use super::blocks;
use crate::plugin::filters::chroma::frame::Region;

/// Separable box mean with clipped windows and linear work per pixel.
fn mean(src: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let mut tmp = vec![0.0; w * h];
    let mut out = tmp.clone();
    for y in 0..h {
        let row = &src[y * w..][..w];
        let mut sum: f32 = row[..radius.min(w - 1) + 1].iter().sum();
        for x in 0..w {
            tmp[y * w + x] = sum / ((x + radius + 1).min(w) - x.saturating_sub(radius)) as f32;
            if x >= radius { sum -= row[x - radius]; }
            if x + radius + 1 < w { sum += row[x + radius + 1]; }
        }
    }
    for x in 0..w {
        let mut sum: f32 = (0..=radius.min(h - 1)).map(|y| tmp[y * w + x]).sum();
        for y in 0..h {
            out[y * w + x] = sum / ((y + radius + 1).min(h) - y.saturating_sub(radius)) as f32;
            if y >= radius { sum -= tmp[(y - radius) * w + x]; }
            if y + radius + 1 < h { sum += tmp[(y + radius + 1) * w + x]; }
        }
    }
    out
}

fn coefficients(guide: &[f32], mask: &[f32], w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
    let mi = mean(guide, w, h, 4);
    let mp = mean(mask, w, h, 4);
    let ii: Vec<_> = guide.iter().map(|i| i * i).collect();
    let ip: Vec<_> = guide.iter().zip(mask).map(|(i, p)| i * p).collect();
    let ci = mean(&ii, w, h, 4);
    let cp = mean(&ip, w, h, 4);
    let mut a = vec![0.0; w * h];
    let mut b = a.clone();
    for n in 0..a.len() {
        a[n] = (cp[n] - mi[n] * mp[n]) / ((ci[n] - mi[n] * mi[n]).max(0.0) + 0.0004);
        b[n] = mp[n] - a[n] * mi[n];
    }
    (mean(&a, w, h, 4), mean(&b, w, h, 4))
}

fn read(src: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let (x, y) = (x.max(0.0), y.max(0.0));
    let (x0, y0) = ((x as usize).min(w - 1), (y as usize).min(h - 1));
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let top = src[y0 * w + x0] * (1.0 - fx) + src[y0 * w + x1] * fx;
    let bottom = src[y1 * w + x0] * (1.0 - fx) + src[y1 * w + x1] * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Full frame luma, including its stride, guides the low resolution mask.
pub fn alpha(
    mask: &[u8], size: (usize, usize), guide: (&[u8], usize),
    frame: (usize, usize), region: Region, curve: &[u8; 256], out: &mut Vec<u8>,
) {
    let (luma, stride) = guide;
    if size.0 == 0 || size.1 == 0 || mask.len() < size.0 * size.1 {
        blocks::alpha(mask, size, frame, region, curve, out);
        return;
    }
    let (w, h) = (frame.0.div_ceil(8), frame.1.div_ceil(8));
    let source: Vec<_> = mask.iter().map(|&v| v as f32 / 255.0).collect();
    let mut guide = vec![0.0; w * h];
    let mut small = guide.clone();
    for y in 0..h {
        for x in 0..w {
            let (px, py) = ((x * 8 + 4).min(frame.0 - 1), (y * 8 + 4).min(frame.1 - 1));
            guide[y * w + x] = luma[py * stride + px] as f32 / 255.0;
            small[y * w + x] = read(&source, size.0, size.1,
                (px as f32 + 0.5) * size.0 as f32 / frame.0 as f32 - 0.5,
                (py as f32 + 0.5) * size.1 as f32 / frame.1 as f32 - 0.5);
        }
    }
    let (a, b) = coefficients(&guide, &small, w, h);
    out.resize(region.w / 2 * (region.h / 2), 0);
    let cols: Vec<_> = (0..region.w / 2).map(|x| {
        let gx = ((region.x + 2 * x + 1) as f32 / 8.0 - 0.5).max(0.0);
        let x0 = (gx as usize).min(w - 1);
        (x0, (x0 + 1).min(w - 1), gx - x0 as f32)
    }).collect();
    for y in 0..region.h / 2 {
        let gy = ((region.y + 2 * y + 1) as f32 / 8.0 - 0.5).max(0.0);
        let y0 = (gy as usize).min(h - 1);
        let y1 = (y0 + 1).min(h - 1);
        let fy = gy - y0 as f32;
        for (x, &(x0, x1, fx)) in cols.iter().enumerate() {
            let (px, py) = (region.x + x * 2, region.y + y * 2);

            let i = (luma[py * stride + px] as f32 + luma[py * stride + px + 1] as f32
                + luma[(py + 1) * stride + px] as f32 + luma[(py + 1) * stride + px + 1] as f32) / 1020.0;
            let interpolate = |c: &[f32]| {
                let top = c[y0 * w + x0] * (1.0 - fx) + c[y0 * w + x1] * fx;
                let bottom = c[y1 * w + x0] * (1.0 - fx) + c[y1 * w + x1] * fx;
                top * (1.0 - fy) + bottom * fy
            };
            let q = interpolate(&a) * i + interpolate(&b);
            out[y * (region.w / 2) + x] = curve[(q * 255.0).round().clamp(0.0, 255.0) as usize];
        }
    }
}

#[cfg(test)]
#[path = "guided_tests.rs"]
mod tests;
