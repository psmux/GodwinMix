//! Transparent pictures on the programme: text, tickers, PNG and SVG with
//! alpha, and video with an alpha channel.
//!
//! The software compositor's output is pinned to I420, and a `compositor`
//! whose output has no alpha refuses every input that has one (checked on
//! GStreamer 1.28.7: linking BGRA into it fails with "can't handle caps").
//! Compositing the whole programme in AYUV instead was measured at 2.6 times
//! the CPU for 1080p30 (`mixer::programme_keeps_alpha`), which a Raspberry
//! Pi cannot pay for a lower third.
//!
//! So a transparent source is drawn after the compositor, by the board, at a
//! cost proportional to the area it covers rather than to the canvas:
//!
//! ```text
//!   sources -> slots -> vmix --[board probe: blend each layer]--> vmix-caps -> encoder
//!                        ^ the layer's own pad: placed by the scene, fed nothing
//! ```
//!
//! The layer keeps a compositor slot like any other item, and the scene writes
//! its place, size, alpha and z order there as it always does. Its pictures
//! never reach that pad (the carrier is dropped at the head of its programme
//! branch), so the compositor skips it, and the board reads the pad's
//! properties back each frame to know where to draw. A transition fading or
//! moving the item drives those same properties, so the layer follows.
//!
//! What this cannot do: a transparent item is drawn over every opaque item,
//! whatever its place in the stack, because the opaque ones are all already
//! inside the frame the board draws on. Among transparent items the stack
//! order holds. Crop and rotation on the item are not applied to it. On a GPU
//! graphics entry the board does not draw at all, and the carrier, the
//! picture flattened on grey, goes through the compositor instead.

pub mod blend;
pub mod board;
pub mod carrier;
pub mod clock;
pub mod draw;
pub mod layer;
pub mod picture;
pub mod place;
pub mod worker;

pub use board::Board;
pub use layer::Layer;
pub use picture::{Direction, Motion, Picture};
