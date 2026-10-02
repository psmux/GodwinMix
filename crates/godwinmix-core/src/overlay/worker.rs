//! The thread a text, a ticker or an SVG renders on.
//!
//! Rendering takes milliseconds and happens only when something changed: the
//! params, or the size the item is drawn at. Neither the mixer thread nor a
//! streaming thread waits for it. The board hears a new size on the
//! programme's streaming thread and only drops a message here; `configure`
//! drops one too and answers at once. Between changes this thread sleeps on
//! its channel and costs nothing.
//!
//! Sizes are coalesced: an item being dragged or animated reports a new size
//! every frame, and rendering each of them would be wasted work, so the
//! thread waits until the size has been still for `SETTLE` (or `SETTLE_MAX`
//! has gone by) before it renders. Until then the board stretches the last
//! picture, which is what it looked like a moment ago anyway.

use super::carrier::Carrier;
use super::layer::Layer;
use super::picture::{Motion, Picture};
use anyhow::Result;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;

const SETTLE: Duration = Duration::from_millis(60);
const SETTLE_MAX: Duration = Duration::from_millis(400);

/// What a render produced.
pub struct Rendered {
    pub picture: Option<Picture>,
    pub motion: Motion,
    /// A still drawn under a crawl. See `Layer::set_backdrop`.
    pub backdrop: Option<Picture>,
}

/// What the thread can be told.
pub enum Msg<P> {
    /// New params. `restart` starts a crawl again from its edge.
    Set(P, bool),
    /// The size the board is drawing the item at.
    Drawn((u32, u32)),
    Stop,
}

pub type RenderFn<P> = fn(&P, Option<(u32, u32)>) -> Result<Rendered>;

/// Start the thread, rendering `first` straight away. The layer's resize
/// callback is pointed at it, so the board's sizes arrive here.
pub fn spawn<P: Send + 'static>(name: &str, first: P, layer: Arc<Layer>, carrier: Arc<Carrier>, render: RenderFn<P>) -> Sender<Msg<P>> {
    let (tx, rx) = channel::<Msg<P>>();
    let resized = tx.clone();
    layer.on_resize(move |size| {
        let _ = resized.send(Msg::Drawn(size));
    });
    let spawned = std::thread::Builder::new().name(format!("gmx-render-{name}")).spawn(move || {
        let mut state = first;
        let mut drawn: Option<(u32, u32)> = None;
        let (mut dirty, mut content) = (true, true);
        loop {
            if dirty {
                publish(&layer, &carrier, render(&state, drawn), content);
                (dirty, content) = (false, false);
            }
            let Ok(first) = rx.recv() else { return };
            let started = Instant::now();
            let mut next = Some(first);
            while let Some(msg) = next.take() {
                match msg {
                    Msg::Set(p, restart) => {
                        state = p;
                        if restart {
                            layer.restart();
                        }
                        (dirty, content) = (true, true);
                    }
                    Msg::Drawn(size) => {
                        dirty |= drawn != Some(size);
                        drawn = Some(size);
                    }
                    Msg::Stop => return,
                }
                let left = SETTLE_MAX.saturating_sub(started.elapsed());
                next = match rx.recv_timeout(SETTLE.min(left)) {
                    Ok(m) => Some(m),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                };
            }
        }
    });
    if let Err(e) = spawned {
        warn!(error = %e, "could not start a render thread");
    }
    tx
}

fn publish(layer: &Layer, carrier: &Carrier, rendered: Result<Rendered>, content: bool) {
    match rendered {
        Ok(r) => {
            let picture = r.picture.map(Arc::new);
            if content {
                carrier.show(picture.as_deref());
            }
            layer.set_backdrop(r.backdrop.map(Arc::new));
            layer.set_motion(r.motion);
            layer.set_picture(picture);
        }
        Err(e) => warn!(error = %e, "could not render a text or picture; the last one stays on screen"),
    }
}
