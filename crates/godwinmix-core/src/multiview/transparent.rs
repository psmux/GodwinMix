//! Text, tickers and transparent pictures in the scene preview.
//!
//! On the programme these are drawn by the board over the compositor's output
//! (`crate::overlay`), and what reaches their compositor pad is a carrier: the
//! picture flattened on grey at the canvas size. The preview used to show
//! that carrier, fitted into the item's box, so a lower third that fills its
//! box on air looked a fraction of the size in the preview. The preview now
//! does what the programme does: a board of its own over its own compositor,
//! drawing the same layers, and the carrier held back from its pad.
//!
//! The preview's board borrows each layer from the programme's and draws the
//! picture it already holds, scaled to the preview's box. It does not report
//! the size it draws at, so nothing is rendered again for it: a preview costs
//! the blend of a thumbnail sized picture and nothing more.

use std::sync::Arc;

use gstreamer as gst;

use crate::overlay::board::hold_back_at;
use crate::overlay::Board;
use crate::plugin::branch::VideoPads;

pub struct Transparent {
    board: Arc<Board>,
    /// The programme's board, where the layers live. None until the mixer
    /// hands it over, and in a mosaic built without one.
    layers: Option<Arc<Board>>,
}

impl Transparent {
    pub fn new(compositor: &gst::Element, size: (i32, i32)) -> Transparent {
        Transparent { board: Board::watching(compositor, size), layers: None }
    }

    pub fn borrow(&mut self, layers: Option<Arc<Board>>) {
        self.layers = layers;
    }

    /// Draw `source` over `pad` if it is a transparent one.
    pub fn bind(&self, source: &str, pad: &gst::Pad) {
        let Some(layer) = self.layers.as_ref().and_then(|b| b.layer_of(source)) else { return };
        let pads = VideoPads::new();
        pads.attach(pad);
        hold_back_at(pad, layer.clone());
        self.board.attach(source, layer, pads);
    }

    pub fn unbind(&self, source: &str) {
        self.board.detach(source);
    }
}
