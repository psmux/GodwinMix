//! VISCA: Sony's camera control protocol, over IP (UDP 52381 with Sony's
//! header) or as raw bytes over TCP.
//!
//! * [`command`]: the bytes of each command and inquiry.
//! * [`reply`]: reading what comes back.
//! * [`ip`]: the VISCA over IP header.
//! * [`udp`], [`tcp`]: the two links, behind [`link::Link`].

pub mod command;
pub mod ip;
pub mod link;
mod ranges;
pub mod reply;
pub mod tcp;
pub mod udp;

use std::net::SocketAddr;
use std::time::Duration;

use crate::error::{Code, Error, Result};
use crate::types::{Capabilities, Exposure, Focus, Position, Velocity};
use crate::{check_preset, Ptz};
pub use command::Ranges;
use link::Link;

pub struct Visca {
    link: Box<dyn Link>,
    addr: u8,
    ranges: Ranges,
    protocol: &'static str,
    /// The last pan and tilt and zoom messages sent, so a loop that sends the
    /// same speed ten times a second sends it once.
    last_pan_tilt: Vec<u8>,
    last_zoom: Vec<u8>,
}

impl Visca {
    /// Sony's VISCA over IP on UDP, normally port 52381.
    pub fn over_ip(to: SocketAddr, ranges: Ranges, timeout: Duration) -> Result<Visca> {
        Ok(Visca::with_link(Box::new(udp::IpLink::connect(to, timeout)?), ranges, "visca-ip"))
    }

    /// Raw VISCA over TCP.
    pub fn over_tcp(to: SocketAddr, ranges: Ranges, timeout: Duration) -> Visca {
        Visca::with_link(Box::new(tcp::TcpLink::new(to, timeout)), ranges, "visca-tcp")
    }

    pub fn with_link(link: Box<dyn Link>, ranges: Ranges, protocol: &'static str) -> Visca {
        Visca { link, addr: 1, ranges, protocol, last_pan_tilt: Vec::new(), last_zoom: Vec::new() }
    }

    fn command(&mut self, m: Vec<u8>) -> Result<()> {
        self.link.exchange(&m, false).map(|_| ())
    }

    fn inquire(&mut self, m: Vec<u8>, want: usize) -> Result<Vec<u8>> {
        let data = self.link.exchange(&m, true)?;
        if data.len() < want {
            return Err(Error::new(Code::BadReply, format!("{} answered an inquiry with {} bytes where {want} were expected. It may not have this inquiry.", self.link.name(), data.len())));
        }
        Ok(data)
    }

    /// Send a movement unless it is the one already in force. A failure
    /// forgets it, so the next call sends it again.
    fn movement(&mut self, m: Vec<u8>, zoom: bool) -> Result<()> {
        let last = if zoom { &self.last_zoom } else { &self.last_pan_tilt };
        if *last == m {
            return Ok(());
        }
        let out = self.command(m.clone());
        let keep = if out.is_ok() { m } else { Vec::new() };
        if zoom { self.last_zoom = keep } else { self.last_pan_tilt = keep }
        out
    }
}

impl Ptz for Visca {
    fn protocol(&self) -> &'static str {
        self.protocol
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { absolute: true, position: true, focus: true, exposure: true, presets: 256 }
    }

    fn move_at(&mut self, v: Velocity) -> Result<()> {
        self.movement(command::pan_tilt(self.addr, v.pan, v.tilt, &self.ranges), false)?;
        self.movement(command::zoom(self.addr, v.zoom), true)
    }

    fn zoom(&mut self, speed: f32) -> Result<()> {
        self.movement(command::zoom(self.addr, speed), true)
    }

    fn focus(&mut self, f: Focus) -> Result<()> {
        self.command(match f {
            Focus::Auto => command::focus_auto(self.addr, true),
            Focus::Manual => command::focus_auto(self.addr, false),
            Focus::OnePush => command::focus_one_push(self.addr),
            Focus::Drive(speed) => command::focus_drive(self.addr, speed),
        })
    }

    fn goto_preset(&mut self, n: u16) -> Result<()> {
        check_preset(n, &self.capabilities())?;
        self.last_pan_tilt.clear();
        self.last_zoom.clear();
        self.command(command::preset(self.addr, 0x02, n))
    }

    fn save_preset(&mut self, n: u16) -> Result<()> {
        check_preset(n, &self.capabilities())?;
        self.command(command::preset(self.addr, 0x01, n))
    }

    fn position(&mut self) -> Result<Position> {
        let pt = self.inquire(command::inquire_pan_tilt(self.addr), 8)?;
        let z = self.inquire(command::inquire_zoom(self.addr), 4)?;
        let word = |b: &[u8]| reply::from_nibbles(b).unwrap_or(0);
        Ok(self.ranges.normalise(word(&pt[0..4]) as i16 as i32, word(&pt[4..8]) as i16 as i32, word(&z[0..4]) as i32))
    }

    fn goto(&mut self, p: Position) -> Result<()> {
        let (pan, tilt, zoom) = self.ranges.steps(&p);
        self.last_pan_tilt.clear();
        self.last_zoom.clear();
        self.command(command::pan_tilt_absolute(self.addr, pan, tilt, &self.ranges))?;
        self.command(command::zoom_absolute(self.addr, zoom))
    }

    fn exposure(&mut self, e: Exposure) -> Result<()> {
        if let Some(mode) = e.mode {
            self.command(command::exposure_mode(self.addr, mode))?;
        }
        for (which, value) in [(command::IRIS, e.iris), (command::SHUTTER, e.shutter), (command::GAIN, e.gain)] {
            if let Some(v) = value {
                self.command(command::exposure_direct(self.addr, which, v))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
