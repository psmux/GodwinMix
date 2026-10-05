//! What a take or a firing needs from a library item, read once, so the
//! mixer thread never touches the library and two takes of the same item
//! cannot see it half edited.

use crate::overlay::modes::Mode;
use anyhow::{Context, Result};
use godwinmix_protocol::fx::{FxBlend, FxKind, FxManifest};
use std::path::{Path, PathBuf};

/// One item, ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub name: String,
    pub look: Look,
    /// How long it runs: a clip's own length, or what the take asked for.
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Look {
    /// A clip over both scenes by `mode`, with the cut at `cut_at_ms`.
    Clip { path: PathBuf, mode: Mode, cut_at_ms: u64 },
    /// A black to white picture; `softness` in thousandths.
    Matte { path: PathBuf, softness: u32, invert: bool },
    /// GLSL in the gl-transitions form, and the name a software version is
    /// found under.
    Shader { name: String, source: String },
}

impl Look {
    /// What a sentence calls it.
    pub fn word(&self) -> &'static str {
        match self {
            Look::Clip { .. } => "clip",
            Look::Matte { .. } => "luma matte",
            Look::Shader { .. } => "shader transition",
        }
    }
}

pub fn mode_of(blend: FxBlend) -> Mode {
    match blend {
        FxBlend::Normal => Mode::Normal,
        FxBlend::Screen => Mode::Screen,
        FxBlend::Add => Mode::Add,
        FxBlend::Luma => Mode::Luma,
    }
}

impl Plan {
    /// The plan for the item `m` in folder `dir`. `duration_ms` is what a
    /// take asked for, which a matte or a shader runs for; a clip runs for
    /// its own length whatever was asked.
    pub fn of(m: &FxManifest, dir: &Path, duration_ms: Option<u64>) -> Result<Plan> {
        let path = dir.join(&m.file);
        let ceiling = crate::mixer::transition::MAX_DURATION_MS;
        let look = match m.kind {
            FxKind::Stinger | FxKind::Overlay => {
                let cut = m.cut_at_ms.or(m.cut_at_measured_ms).unwrap_or(m.duration_ms / 2);
                Look::Clip { path, mode: mode_of(m.blend), cut_at_ms: cut.min(m.duration_ms) }
            }
            FxKind::Matte => Look::Matte {
                path,
                softness: (m.softness.unwrap_or(0.1).clamp(0.0, 1.0) * 1000.0).round() as u32,
                invert: m.invert,
            },
            FxKind::Shader => {
                let source = std::fs::read_to_string(&path).with_context(|| format!("reading the shader {}", path.display()))?;
                Look::Shader { name: m.name.clone(), source }
            }
        };
        let duration_ms = match &look {
            Look::Clip { .. } => m.duration_ms,
            _ => duration_ms.unwrap_or(m.duration_ms),
        }
        .clamp(1, ceiling);
        Ok(Plan { name: m.name.clone(), look, duration_ms })
    }

    /// When the scenes swap, from the start: a clip's cut, or the start of
    /// the window for a matte or a shader, which draw the old scene back.
    pub fn cut_at_ms(&self) -> u64 {
        match &self.look {
            Look::Clip { cut_at_ms, .. } => *cut_at_ms,
            _ => 0,
        }
    }
}
