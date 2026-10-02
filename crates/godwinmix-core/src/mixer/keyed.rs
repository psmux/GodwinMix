//! Filters whose result has alpha, drawn by the overlay board.
//!
//! A chroma key on a scene item is the case this exists for. The compositor's
//! output is I420 and takes no alpha, so a keyed picture fed to its pad would
//! arrive with its clear parts black. The board draws after the compositor
//! and does take alpha, so the key hands it the picture instead and sends the
//! pad nothing; the pad keeps its place, size, z order and fades, and the
//! board reads them off it each frame like it does for a text or a PNG.
//!
//! That also settles the stacking. The board draws every transparent item
//! over the opaque ones, in z order among themselves, and a keyed presenter
//! is one of them: a background picture under it stays under it, and a desk
//! PNG above it in the scene stays in front of it.
//!
//! On a GPU compositor, which takes alpha itself but is not drawn on by the
//! board, nothing is attached and the key flattens over black instead.

use crate::overlay::Board;
use crate::plugin::branch::VideoPads;
use crate::plugin::filter::BoardHook;
use gstreamer as gst;
use std::sync::Arc;

/// Draw a filter's result on `board` wherever `pad` is drawn, if it has one
/// to draw and the board can draw on this programme.
pub fn attach(board: Option<&Arc<Board>>, hook: Option<BoardHook>, pad: &gst::Pad, key: &str) {
    let pads = VideoPads::new();
    pads.track(pad);
    attach_at(board, hook, pads, key);
}

/// The same for a set of pads: every place one source is drawn, for a key on
/// that source's programme branch.
pub fn attach_at(board: Option<&Arc<Board>>, hook: Option<BoardHook>, pads: Arc<VideoPads>, key: &str) {
    let (Some(board), Some(hook)) = (board, hook) else { return };
    if super::programme_keeps_alpha() {
        return;
    }
    hook.draw_at(pads.clone());
    board.attach(key, hook.layer.clone(), pads);
}

/// Stop drawing what was attached under `key`.
pub fn detach(board: Option<&Arc<Board>>, key: &str) {
    if let Some(board) = board {
        board.detach(key);
    }
}
