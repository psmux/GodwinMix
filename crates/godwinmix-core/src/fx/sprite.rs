//! The moving preview: twelve frames of the item at 128x72 side by side in
//! one JPEG, over a blue scene going to an orange one.
//!
//! Made once, when an item is imported or first asked for, by the same
//! blend, matte and shader code the programme uses, and kept beside the item
//! as `preview-strip.jpg`, with its middle frame as `preview.jpg`, the
//! still the gallery shows for a transition or an effect. A picker animates it with a CSS step animation, so the
//! browser does the moving and only while the picker is open; the mixer
//! never renders a preview on a clock.

use super::detect::{self, Measured, SMALL};
use super::frame::Pic;
use super::plan::{Look, Plan};
use super::matte::dissolve;
use super::shader::{gl::Gl, probe};
use super::Mix;
use crate::overlay::blend::{Draw, Planes, Rect, Source};
use crate::overlay::modes;
use anyhow::{Context, Result};
use godwinmix_protocol::fx::FxManifest;
use std::path::{Path, PathBuf};

pub const FRAMES: u32 = 12;
const W: usize = SMALL.0 as usize;
const H: usize = SMALL.1 as usize;

/// Where an item's strip is kept.
pub fn path(dir: &Path) -> PathBuf {
    dir.join("preview-strip.jpg")
}

/// Write the strip and the still beside the item.
pub fn write(dir: &Path, (strip, still): (Vec<u8>, Vec<u8>)) -> Result<()> {
    std::fs::write(path(dir), strip).with_context(|| format!("writing the preview in {}", dir.display()))?;
    std::fs::write(dir.join("preview.jpg"), still).with_context(|| format!("writing the still in {}", dir.display()))
}

/// The strip for the item in `dir`, made now if it is missing or older than
/// the item's manifest.
pub fn ensure(m: &FxManifest, dir: &Path) -> Result<PathBuf> {
    let out = path(dir);
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if out.is_file() && modified(&out) >= modified(&dir.join(super::library::MANIFEST)) {
        return Ok(out);
    }
    write(dir, render(m, dir, None)?)?;
    Ok(out)
}

/// One I420 frame of the preview size, a horizontal ramp in one hue.
fn scene(u: u8, v: u8, lo: u8) -> [Vec<u8>; 3] {
    let y = (0..H).flat_map(|_| (0..W).map(move |x| lo + (x * 60 / W) as u8)).collect();
    [y, vec![u; W * H / 4], vec![v; W * H / 4]]
}

/// Render the strip and its middle frame, as two JPEGs.
pub fn render(m: &FxManifest, dir: &Path, measured: Option<&Measured>) -> Result<(Vec<u8>, Vec<u8>)> {
    let plan = Plan::of(m, dir, None)?;
    let owned;
    let clip = match (&plan.look, measured) {
        (Look::Clip { path, .. }, None) => {
            owned = detect::measure(path)?;
            Some(&owned)
        }
        (Look::Clip { .. }, Some(m)) => Some(m),
        _ => None,
    };
    let mix: Option<Box<dyn Mix>> = match &plan.look {
        Look::Matte { path, softness, invert } => {
            let plane = super::matte::decode(path, (W as i32, H as i32))?;
            Some(Box::new(super::matte::Matte::from_plane(plane, W, *softness as f64 / 1000.0, *invert)))
        }
        Look::Shader { name, .. } => Some(Box::new(Software(super::shader::cpu::find(name)))),
        Look::Clip { .. } => None,
    };
    // A shader is drawn the way it will run: on the GPU where GL runs, here
    // on this thread and stopped before the strip is written.
    let gpu = match &plan.look {
        Look::Shader { source, .. } if probe::available() => super::shader::fragment(source).and_then(|f| Gl::start(&f, (W as i32, H as i32))).ok(),
        _ => None,
    };
    let (blue, orange) = (scene(160, 100, 40), scene(70, 190, 70));
    let mut strip = vec![0u8; W * FRAMES as usize * H * 3];
    for i in 0..FRAMES as usize {
        let t = i as f64 / (FRAMES - 1) as f64;
        let after = clip.is_none() || (m.transition && (t * plan.duration_ms as f64) as u64 >= plan.cut_at_ms());
        let mut f = if after { orange.clone() } else { blue.clone() };
        let [fy, fu, fv] = &mut f;
        let mut planes = Planes { y: fy, u: fu, v: fv, strides: [W, W / 2, W / 2], width: W as i32, height: H as i32 };
        if let (Some(c), Look::Clip { mode, .. }) = (clip, &plan.look) {
            let k = ((t * c.frames.len() as f64) as usize).min(c.frames.len() - 1);
            let whole = Rect::new(0, 0, W as i32, H as i32);
            modes::draw(&mut planes, &Source { data: &c.frames[k], stride: W * 4 }, &Draw { window: whole, to: whole, clip: whole, alpha: 255 }, *mode);
        }
        let old = Pic { y: &blue[0], u: &blue[1], v: &blue[2], strides: [W, W / 2, W / 2] };
        match (&gpu, &mix) {
            (Some(gl), _) => {
                probe::settle(gl, &old, &mut planes, t, std::time::Duration::from_secs(1));
            }
            (None, Some(mix)) => mix.mix(&old, &mut planes, t),
            _ => {}
        }
        rgb(&f, &mut strip, i);
    }
    if let Some(gl) = gpu {
        gl.close();
    }
    let mid = FRAMES as usize / 2;
    let row = W * FRAMES as usize * 3;
    let still: Vec<u8> = (0..H).flat_map(|y| strip[y * row + mid * W * 3..][..W * 3].to_vec()).collect();
    Ok((jpeg(&strip, W as u32 * FRAMES)?, jpeg(&still, W as u32)?))
}

fn jpeg(rgb: &[u8], width: u32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
        .encode(rgb, width, H as u32, image::ExtendedColorType::Rgb8)
        .context("encoding the preview")?;
    Ok(out)
}

/// One I420 frame into its place in the RGB strip, BT.709 limited range.
fn rgb(f: &[Vec<u8>; 3], strip: &mut [u8], index: usize) {
    let row = W * FRAMES as usize * 3;
    for y in 0..H {
        for x in 0..W {
            let l = (f[0][y * W + x] as f64 - 16.0) * 1.164;
            let (u, v) = (f[1][(y / 2) * (W / 2) + x / 2] as f64 - 128.0, f[2][(y / 2) * (W / 2) + x / 2] as f64 - 128.0);
            let px = [l + 1.793 * v, l - 0.213 * u - 0.533 * v, l + 2.112 * u];
            let at = y * row + (index * W + x) * 3;
            for (c, value) in px.iter().enumerate() {
                strip[at + c] = value.clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// A shader's software version for the strip, or a dissolve.
struct Software(Option<super::shader::cpu::Shader>);

impl Mix for Software {
    fn mix(&self, old: &Pic<'_>, f: &mut Planes<'_>, t: f64) {
        match self.0 {
            Some(s) => s(old, f, t),
            None => dissolve(old, f, t),
        }
    }
}
