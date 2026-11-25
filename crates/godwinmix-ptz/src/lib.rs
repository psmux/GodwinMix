//! `godwinmix-ptz`: PTZ camera drivers behind one trait.
//!
//! | Module | What it is |
//! |---|---|
//! | [`visca`] | Sony VISCA over IP (UDP 52381) and raw VISCA over TCP |
//! | [`onvif`] | ONVIF PTZ and imaging, with the SOAP and WS-Security code ipcam shares |
//! | [`canon`] | Canon's HTTP control for the CR-N series |
//! | [`fake`] | cameras that keep a position and answer, for tests and demos |
//! | [`http`] | one request, one answer, a deadline, no client crate |
//!
//! Every driver is blocking and keeps its own socket. A caller that must not
//! wait (the plugin's follow loop) runs it on a thread of its own.

pub mod canon;
pub mod error;
pub mod fake;
pub mod http;
pub mod onvif;
pub mod open;
pub mod types;
pub mod visca;

pub use error::{Code, Error, Result};
pub use open::{open, Protocol, Target};
pub use types::{Capabilities, Exposure, ExposureMode, Focus, Position, Velocity};

/// One PTZ camera. `move_at` is `move` in the plan; `move` is a keyword.
pub trait Ptz: Send {
    /// The protocol's slug: `visca-ip`, `visca-tcp`, `onvif`, `canon`, `fake`.
    fn protocol(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    /// Pan, tilt and zoom at these speeds until told otherwise.
    fn move_at(&mut self, v: Velocity) -> Result<()>;
    /// Stop pan, tilt and zoom.
    fn stop(&mut self) -> Result<()> {
        self.move_at(Velocity::STOP)
    }
    /// Zoom at a speed, -1 (wider) to 1 (tighter). 0 stops the zoom only.
    fn zoom(&mut self, speed: f32) -> Result<()>;
    fn focus(&mut self, f: Focus) -> Result<()>;
    /// Recall preset `n`, numbered from 1.
    fn goto_preset(&mut self, n: u16) -> Result<()>;
    /// Store the current shot as preset `n`, numbered from 1.
    fn save_preset(&mut self, n: u16) -> Result<()>;
    /// Where the camera points now.
    fn position(&mut self) -> Result<Position>;
    /// Go to a position, normalised as `Position` is.
    fn goto(&mut self, p: Position) -> Result<()>;
    fn exposure(&mut self, _e: Exposure) -> Result<()> {
        Err(Error::unsupported(self.protocol(), "exposure"))
    }
    /// A second driver on the same camera, safe to use from another thread
    /// while this one is busy, or `None` when the protocol cannot take two
    /// at once. The plugin moves the camera through one and sends the slow
    /// commands (presets, position, focus) through the other, so a move
    /// never waits behind them. HTTP protocols can; VISCA cannot, since the
    /// camera keeps one sequence count and many take one TCP link only.
    fn second(&self) -> Option<Box<dyn Ptz>> {
        None
    }
}

/// Preset numbers are 1 based everywhere in GodwinMix. A 0 or one past what
/// the protocol holds is refused with the range in the message.
pub fn check_preset(n: u16, caps: &Capabilities) -> Result<()> {
    if n == 0 || n > caps.presets {
        return Err(Error::new(
            Code::BadArgument,
            format!("preset {n} is out of range. This camera's presets are 1 to {}.", caps.presets),
        ));
    }
    Ok(())
}
