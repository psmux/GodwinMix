//! Transitions and effects from packs: what an editor's light leak, bokeh,
//! glitch, film burn, luma wipe or shader transition becomes on a live
//! programme.
//!
//! ```text
//!   graphics/<name>/graphic.toml + clip ---import, measure, preview---> library
//!   take {transition: "<name>"} --plan--> curves (the cut) + a pass on the board
//!   fx.fire {name}               --plan--> a pass on the board, until the clip ends
//! ```
//!
//! Everything here is drawn by the overlay board after the compositor, as a
//! pass (`overlay::pass`). Nothing is built until a take or a firing asks:
//! a clip is decoded on a pipeline of its own for as long as it plays, a
//! matte is read once and kept, a shader starts GStreamer GL for the length
//! of its window. With none of them running the board's probe is not even
//! on the pad.
//!
//! The library is the gallery's folder, `graphics/` in the media library,
//! one folder an item with a `graphic.toml` whose kind is `transition` or
//! `effect`, so the gallery, a file manager and an agent all see the same thing. See
//! `docs/reference/fx.md`.

pub mod assign;
pub mod clip;
pub mod detect;
pub mod frame;
pub mod gallery_import;
pub mod import;
pub mod library;
pub mod matte;
pub mod player;
pub mod plan;
pub mod shader;
pub mod sprite;
pub mod starter;
pub mod toml_form;

pub use plan::{Look, Plan};

use crate::overlay::blend::Planes;
use crate::overlay::pass::Pass;
use crate::overlay::Board;
use anyhow::Result;
use frame::{FramePass, Outgoing, Pic};
use std::sync::{Arc, Weak};

/// A transition worked on two pictures: draw the old one back over the new
/// one, already in `f`, at progress `t`.
pub trait Mix: Send + Sync {
    fn mix(&self, old: &Pic<'_>, f: &mut Planes<'_>, t: f64);
}

/// A look on the board: what to take off when it is over.
pub struct Running {
    pass: u64,
    player: Option<player::Player>,
    board: Weak<Board>,
}

impl Running {
    /// Take it off the programme now, wherever it had got to.
    pub fn stop(self) {
        if let Some(p) = &self.player {
            p.stop();
        }
        if let Some(board) = self.board.upgrade() {
            board.remove_pass(self.pass);
        }
    }
}

/// Play a clip over the programme once, on its own: `fx.fire`.
pub fn fire(plan: &Plan, board: &Arc<Board>, canvas: (i32, i32), opacity: f64) -> Result<()> {
    let Look::Clip { path, mode, .. } = &plan.look else {
        anyhow::bail!("{} is a {}, which changes one scene into another and cannot play on its own. Fire an overlay or a stinger, or use it in a take", plan.name, plan.look.word())
    };
    let player = player::Player::start(path, decode_size(*mode, canvas))?;
    let pass: Arc<dyn Pass> = Arc::new(clip::ClipPass::new(&plan.name, &player, *mode, opacity, None, plan.duration_ms));
    let id = board.add_pass(pass);
    let weak = Arc::downgrade(board);
    player.on_end(move || {
        if let Some(b) = weak.upgrade() {
            b.remove_pass(id);
        }
    });
    Ok(())
}

/// Put a transition's pass on the board for `window`, in running time.
/// A clip's player may be handed in already started, which is how the mixer
/// waits for a clip's first frame before it binds the cut.
pub fn transition(plan: &Plan, board: &Arc<Board>, canvas: (i32, i32), window: (u64, u64), easing: crate::mixer::transition::Easing, outgoing: Outgoing, started: Option<player::Player>) -> Result<Running> {
    let (pass, player): (Arc<dyn Pass>, _) = match &plan.look {
        Look::Clip { path, mode, .. } => {
            let player = match started {
                Some(p) => p,
                None => player::Player::start(path, decode_size(*mode, canvas))?,
            };
            let pass = clip::ClipPass::new(&plan.name, &player, *mode, 1.0, Some(window.0), plan.duration_ms);
            (Arc::new(pass), Some(player))
        }
        Look::Matte { path, softness, invert } => {
            let matte = matte::Matte::load(path, canvas, *softness as f64 / 1000.0, *invert);
            (Arc::new(FramePass::new(&plan.name, window, easing, outgoing, Box::new(matte))), None)
        }
        Look::Shader { name, source } => {
            let mix = shader::ShaderMix::start(name, source, canvas);
            (Arc::new(FramePass::new(&plan.name, window, easing, outgoing, Box::new(mix))), None)
        }
    };
    let id = board.add_pass(pass);
    if let Some(p) = &player {
        let weak = Arc::downgrade(board);
        p.on_end(move || {
            if let Some(b) = weak.upgrade() {
                b.remove_pass(id);
            }
        });
    }
    Ok(Running { pass: id, player, board: Arc::downgrade(board) })
}

/// The size a clip is decoded at. A stinger by its alpha keeps the canvas
/// size, because its edges are drawn graphics. Light for Screen, Add or a
/// luma key is soft by nature and is decoded at half the size and stretched,
/// which reads a quarter of the bytes each frame and decodes four times
/// faster, and nobody can see the difference in a light leak.
pub fn decode_size(mode: crate::overlay::modes::Mode, canvas: (i32, i32)) -> (i32, i32) {
    match mode {
        crate::overlay::modes::Mode::Normal => canvas,
        _ => ((canvas.0 / 4 * 2).max(2), (canvas.1 / 4 * 2).max(2)),
    }
}
