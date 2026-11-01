//! Camera log curves, camera gamuts, and a way down to a Rec.709 picture.
//!
//! Each curve is the maker's published decode, from a code value 0 to 1
//! (full range, the way a grading tool hands a LUT its input) to scene
//! linear light, where 0.18 is a grey card. Each gamut is its primaries,
//! all D65, from which the matrix to Rec.709 is worked out here rather than
//! copied in.
//!
//! The way down is a filmic curve (Narkowicz's fit of the ACES one) and a
//! 2.4 display gamma: a grey card lands near 44 percent, white rolls off
//! instead of clipping, and black stays black. It is a sound starting point
//! for a live show, not a camera maker's own look.

/// Sony S-Log3.
pub fn slog3(x: f32) -> f32 {
    if x >= 171.210_3 / 1023.0 {
        10f32.powf((x * 1023.0 - 420.0) / 261.5) * 0.19 - 0.01
    } else {
        (x * 1023.0 - 95.0) * 0.011_25 / (171.210_3 - 95.0)
    }
}

/// Panasonic V-Log.
pub fn vlog(x: f32) -> f32 {
    if x < 0.181 {
        (x - 0.125) / 5.6
    } else {
        10f32.powf((x - 0.598_206) / 0.241_514) - 0.008_73
    }
}

/// Canon C-Log3, with Canon's 0.9 to put a grey card at 0.18.
pub fn clog3(x: f32) -> f32 {
    let lin = if x < 0.097_465_47 {
        -(10f32.powf((0.127_839 - x) / 0.367_268_45) - 1.0) / 14.983_25
    } else if x <= 0.152_778_9 {
        (x - 0.125_122_19) / 1.975_479_8
    } else {
        (10f32.powf((x - 0.122_405_37) / 0.367_268_45) - 1.0) / 14.983_25
    };
    lin * 0.9
}

/// ARRI LogC3 at EI 800.
pub fn logc3(x: f32) -> f32 {
    let (a, b, c, d, e, f) = (5.555_556, 0.052_272, 0.247_19, 0.385_537, 5.367_655, 0.092_809);
    if x > e * 0.010_591 + f {
        (10f32.powf((x - d) / c) - b) / a
    } else {
        (x - f) / e
    }
}

/// Fujifilm F-Log.
pub fn flog(x: f32) -> f32 {
    let (a, b, c, d, e, f) = (0.555_556, 0.009_468, 0.344_676, 0.790_453, 8.735_631, 0.092_864);
    if x >= 0.100_537_77 {
        (10f32.powf((x - d) / c) - b) / a
    } else {
        (x - f) / e
    }
}

/// Nikon N-Log.
pub fn nlog(x: f32) -> f32 {
    if x < 452.0 / 1023.0 {
        (x * 1023.0 / 650.0).powi(3) - 0.0075
    } else {
        ((x * 1023.0 - 619.0) / 150.0).exp()
    }
}

/// HLG, ITU-R BT.2100, scaled so its grey lands at 0.18 like the others.
pub fn hlg(x: f32) -> f32 {
    let (a, b, c) = (0.178_832_77, 0.284_668_92, 0.559_910_73);
    let e = if x <= 0.5 { x * x / 3.0 } else { (((x - c) / a).exp() + b) / 12.0 };
    e * 3.75
}

/// A gamut's red, green and blue primaries as CIE xy.
pub type Primaries = [[f32; 2]; 3];

pub const REC709: Primaries = [[0.64, 0.33], [0.30, 0.60], [0.15, 0.06]];
pub const REC2020: Primaries = [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046]];
pub const S_GAMUT3_CINE: Primaries = [[0.766, 0.275], [0.225, 0.800], [0.089, -0.087]];
pub const V_GAMUT: Primaries = [[0.730, 0.280], [0.165, 0.840], [0.100, -0.030]];
pub const CINEMA_GAMUT: Primaries = [[0.740, 0.270], [0.170, 1.140], [0.080, -0.100]];
pub const ARRI_WG3: Primaries = [[0.6840, 0.3130], [0.2210, 0.8480], [0.0861, -0.1020]];
const D65: [f32; 2] = [0.3127, 0.3290];

type M3 = [[f32; 3]; 3];

fn mul(a: &M3, b: &M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

pub fn apply(m: &M3, v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn inverse(m: &M3) -> M3 {
    let c = |i: usize, j: usize| m[(i + 1) % 3][(j + 1) % 3] * m[(i + 2) % 3][(j + 2) % 3] - m[(i + 1) % 3][(j + 2) % 3] * m[(i + 2) % 3][(j + 1) % 3];
    let det: f32 = (0..3).map(|j| m[0][j] * c(0, j)).sum();
    std::array::from_fn(|i| std::array::from_fn(|j| c(j, i) / det))
}

/// RGB in a gamut to CIE XYZ.
fn to_xyz(p: &Primaries) -> M3 {
    let xyz = |[x, y]: [f32; 2]| [x / y, 1.0, (1.0 - x - y) / y];
    let cols = p.map(xyz);
    let m: M3 = std::array::from_fn(|i| std::array::from_fn(|j| cols[j][i]));
    let s = apply(&inverse(&m), xyz(D65));
    std::array::from_fn(|i| std::array::from_fn(|j| m[i][j] * s[j]))
}

/// The matrix from linear RGB in `from` to linear Rec.709.
pub fn to_rec709(from: &Primaries) -> M3 {
    mul(&inverse(&to_xyz(&REC709)), &to_xyz(from))
}

/// Scene linear Rec.709 to a display picture, 0 to 1.
pub fn display(lin: f32) -> f32 {
    let x = lin.max(0.0) * 0.6;
    let mapped = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
    mapped.clamp(0.0, 1.0).powf(1.0 / 2.4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32, tol: f32) {
        assert!((a - b).abs() < tol, "{a} is not {b}");
    }

    #[test]
    fn every_curve_puts_its_published_grey_at_a_grey_card() {
        near(slog3(420.0 / 1023.0), 0.18, 0.001);
        near(vlog(0.423_3), 0.18, 0.002);
        near(clog3(0.343_3), 0.18, 0.003);
        near(logc3(0.391_007), 0.18, 0.001);
        near(flog(0.459_32), 0.18, 0.002);
        near(nlog(0.363_7), 0.18, 0.002);
    }

    #[test]
    fn the_curves_rise_everywhere() {
        for f in [slog3, vlog, clog3, logc3, flog, nlog, hlg] {
            let mut last = f32::MIN;
            for i in 0..=1000 {
                let v = f(i as f32 / 1000.0);
                assert!(v >= last, "falls at {i}");
                last = v;
            }
        }
    }

    #[test]
    fn rec709_to_itself_is_the_identity() {
        let m = to_rec709(&REC709);
        for i in 0..3 {
            for j in 0..3 {
                near(m[i][j], if i == j { 1.0 } else { 0.0 }, 1e-4);
            }
        }
    }

    #[test]
    fn white_stays_white_through_every_gamut() {
        for p in [REC2020, S_GAMUT3_CINE, V_GAMUT, CINEMA_GAMUT, ARRI_WG3] {
            let w = apply(&to_rec709(&p), [1.0; 3]);
            w.iter().for_each(|&c| near(c, 1.0, 1e-3));
        }
    }

    #[test]
    fn a_grey_card_shows_near_the_middle_and_white_rolls_off() {
        near(display(0.18), 0.44, 0.02);
        assert!(display(1.0) < 0.9 && display(1.0) > 0.8);
        assert!(display(16.0) > 0.99);
        assert_eq!(display(0.0), 0.0);
    }
}
