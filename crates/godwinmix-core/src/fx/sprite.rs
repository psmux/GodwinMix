//! The moving preview: twelve frames of the item at 128x72 side by side in
//! one JPEG, over a blue scene going to an orange one.
//!
//! Made once, when an item is imported or first asked for, by the same
//! blend, matte and shader code the programme uses, and kept beside the item
//! as `.preview.jpg`. A picker animates it with a CSS step animation, so the
//! browser does the moving and only while the picker is open; the mixer
//! never renders a preview on a clock.

use super::detect::{self, Measured, SMALL};
use super::frame::Pic;
use super::plan::{Look, Plan};
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
    dir.join(".preview.jpg")
}

/// The strip for the item in `dir`, made now if it is missing or older than
/// the item's manifest.
pub fn ensure(m: &FxManifest, dir: &Path) -> Result<PathBuf> {
    let out = path(dir);
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if out.is_file() && modified(&out) >= modified(&dir.join(super::library::MANIFEST)) {
        return Ok(out);
    }
    let jpeg = render(m, dir, None)?;
    std::fs::write(&out, jpeg).with_context(|| format!("writing {}", out.display()))?;
    Ok(out)
}

/// One I420 frame of the preview size, a horizontal ramp in one hue.
fn scene(u: u8, v: u8, lo: u8) -> [Vec<u8>; 3] {
    let y = (0..H).flat_map(|_| (0..W).map(move |x| lo + (x * 60 / W) as u8)).collect();
    [y, vec![u; W * H / 4], vec![v; W * H / 4]]
}

/// Render the strip as a JPEG.
pub fn render(m: &FxManifest, dir: &Path, measured: Option<&Measured>) -> Result<Vec<u8>> {
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
        Look::Shader { name, source } => Some(Box::new(super::shader::ShaderMix::start(name, source, (W as i32, H as i32)))),
        Look::Clip { .. } => None,
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
        if let Some(mix) = &mix {
            let old = Pic { y: &blue[0], u: &blue[1], v: &blue[2], strides: [W, W / 2, W / 2] };
            mix.mix(&old, &mut planes, t);
            if matches!(plan.look, Look::Shader { .. }) {
                std::thread::sleep(std::time::Duration::from_millis(40));
                mix.mix(&old, &mut planes, t);
            }
        }
        rgb(&f, &mut strip, i);
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
        .encode(&strip, W as u32 * FRAMES, H as u32, image::ExtendedColorType::Rgb8)
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
