//! The hook the binary registers, so the engine can ask for a piece.
//!
//! The engine finds out a piece is missing (a web page from the config file,
//! with no renderer yet) on the mixer thread, which must never wait. So it
//! only says what it needs; the binary, which owns the builds, starts one on
//! a thread of its own and returns at once. With nothing registered (an
//! embedder, or a test) asking does nothing and the refusal stands.

use std::sync::OnceLock;

type Starter = Box<dyn Fn(&str) + Send + Sync>;

static STARTER: OnceLock<Starter> = OnceLock::new();

/// Called once by the binary at start. A second call is ignored.
pub fn register(start: impl Fn(&str) + Send + Sync + 'static) {
    let _ = STARTER.set(Box::new(start));
}

/// Ask for `piece` to be set up. Returns at once whatever happens.
pub fn need(piece: &str) {
    if let Some(start) = STARTER.get() {
        start(piece);
    }
}
