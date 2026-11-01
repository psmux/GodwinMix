//! Lookup tables for colour: the files a grade comes in, the looks that ship,
//! and the library an operator keeps.
//!
//! A [`Lut`] is what every format reads into: an optional 1D shaper per
//! channel, then an optional 3D cube, both on RGB from 0 to 1. The filter
//! that puts one on a picture is `plugin/filters/lut`; it never reads a file
//! on a frame's thread, it bakes the LUT into a table in the canvas's own
//! Y'CbCr once per change (`bake`) and each frame does a few reads per pixel
//! (`apply`).
//!
//! | module | what |
//! |---|---|
//! | `cube` | `.cube`, the Resolve and Adobe form, 1D, 3D or both |
//! | `threedl` | `.3dl`, the Lustre and Nuke form |
//! | `png` | a picture of a cube: OBS's 512 square, or a Hald CLUT |
//! | `log` | camera log curves and gamuts, for the camera conversions |
//! | `looks` | the built in looks and camera conversions, made in code |
//! | `library` | imported LUTs and saved presets, on disk |
//! | `bake`, `apply` | the table a frame is run through |

pub mod apply;
pub mod bake;
pub mod cube;
pub mod library;
pub mod log;
pub mod looks;
pub mod png;
pub mod preview;
pub mod threedl;

/// The largest cube read: 65 points a side is what Resolve writes at most.
pub const MAX_SIZE: usize = 65;
/// The largest 1D table read.
pub const MAX_SHAPER: usize = 65536;

/// A 1D table per channel over `min..max`.
#[derive(Debug, Clone, PartialEq)]
pub struct Shaper {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub values: Vec<[f32; 3]>,
}

/// A 3D table over `min..max`, red changing fastest, then green, then blue,
/// which is the order `.cube` writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Cube {
    pub size: usize,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub values: Vec<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lut {
    pub title: String,
    pub shaper: Option<Shaper>,
    pub cube: Option<Cube>,
}

impl Lut {
    /// A cube of `size` points a side made by calling `f` at every point.
    pub fn from_fn(title: &str, size: usize, f: impl Fn([f32; 3]) -> [f32; 3]) -> Lut {
        let step = 1.0 / (size - 1) as f32;
        let mut values = Vec::with_capacity(size * size * size);
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    values.push(f([r as f32 * step, g as f32 * step, b as f32 * step]));
                }
            }
        }
        Lut { title: title.into(), shaper: None, cube: Some(Cube { size, min: [0.0; 3], max: [1.0; 3], values }) }
    }

    /// The colour `rgb` becomes: the shaper, then the cube, trilinearly.
    pub fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let mut c = rgb;
        if let Some(s) = &self.shaper {
            c = s.sample(c);
        }
        if let Some(k) = &self.cube {
            c = k.sample(c);
        }
        c
    }

    /// Points a side, or the shaper's length for a 1D only table.
    pub fn size(&self) -> usize {
        match (&self.cube, &self.shaper) {
            (Some(c), _) => c.size,
            (None, Some(s)) => s.values.len(),
            _ => 0,
        }
    }
}

/// Where `x` falls on a table of `n` points over `min..max`: the point below
/// and how far towards the next.
fn place(x: f32, min: f32, max: f32, n: usize) -> (usize, f32) {
    let span = if max > min { max - min } else { 1.0 };
    let t = ((x - min) / span).clamp(0.0, 1.0) * (n - 1) as f32;
    let i = (t.floor() as usize).min(n - 2);
    (i, t - i as f32)
}

impl Shaper {
    pub fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.values.len();
        std::array::from_fn(|ch| {
            let (i, f) = place(rgb[ch], self.min[ch], self.max[ch], n);
            self.values[i][ch] * (1.0 - f) + self.values[i + 1][ch] * f
        })
    }
}

impl Cube {
    pub fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        let (r, fr) = place(rgb[0], self.min[0], self.max[0], n);
        let (g, fg) = place(rgb[1], self.min[1], self.max[1], n);
        let (b, fb) = place(rgb[2], self.min[2], self.max[2], n);
        let at = |dr: usize, dg: usize, db: usize| self.values[(r + dr) + n * ((g + dg) + n * (b + db))];
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        let c00 = lerp(at(0, 0, 0), at(1, 0, 0), fr);
        let c10 = lerp(at(0, 1, 0), at(1, 1, 0), fr);
        let c01 = lerp(at(0, 0, 1), at(1, 0, 1), fr);
        let c11 = lerp(at(0, 1, 1), at(1, 1, 1), fr);
        lerp(lerp(c00, c10, fg), lerp(c01, c11, fg), fb)
    }
}

/// Read a LUT from its bytes, by the file name's extension.
pub fn parse(file_name: &str, bytes: &[u8]) -> anyhow::Result<Lut> {
    let lower = file_name.to_ascii_lowercase();
    let text = || String::from_utf8_lossy(bytes).into_owned();
    if lower.ends_with(".cube") {
        cube::parse(&text())
    } else if lower.ends_with(".3dl") {
        threedl::parse(&text())
    } else if lower.ends_with(".png") {
        png::parse(bytes)
    } else {
        anyhow::bail!("{file_name} is not a LUT this mixer reads. It reads {}", FORMATS.join(", "))
    }
}

/// What `parse` reads, for an error to list.
pub const FORMATS: &[&str] = &[
    ".cube (Resolve, Adobe, most camera makers: 1D, 3D or both, up to 65 points)",
    ".3dl (Lustre, Nuke, Flame)",
    ".png (OBS's 512 x 512 square, or a Hald CLUT)",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identity_cube_gives_back_what_it_is_given() {
        let lut = Lut::from_fn("id", 17, |c| c);
        for c in [[0.1, 0.5, 0.9], [0.0, 1.0, 0.33], [0.77, 0.21, 0.6]] {
            let out = lut.sample(c);
            for i in 0..3 {
                assert!((out[i] - c[i]).abs() < 1e-5, "{c:?} came back {out:?}");
            }
        }
    }

    #[test]
    fn outside_the_domain_is_held_at_the_edge() {
        let lut = Lut::from_fn("id", 2, |c| c);
        assert_eq!(lut.sample([-0.5, 1.5, 0.5]), [0.0, 1.0, 0.5]);
    }
}
