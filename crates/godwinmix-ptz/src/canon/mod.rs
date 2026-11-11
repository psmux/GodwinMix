//! Canon's XC protocol, the HTTP web API of the CR-N series PTZ cameras
//! (CR-N100, CR-N300, CR-N350, CR-N400, CR-N500, CR-N700) and the CR-X300.
//! Every control is a GET on `/-wvhttp-01-/control.cgi`, a query on
//! `info.cgi`, or a preset store on `preset/set`. The README says which
//! sources this follows and what is not yet checked against a camera.
//!
//! * [`client`]: the request, the login and the status header.
//! * [`auth`], [`md5`]: Basic and Digest logins.

pub mod auth;
pub mod client;
pub mod md5;
mod ranges;

pub use ranges::Ranges;
use ranges::{denorm, norm};

use std::time::Duration;

use crate::error::{Code, Error, Result};
use crate::types::{Capabilities, Exposure, ExposureMode, Focus, Position, Velocity, DEAD};
use crate::{check_preset, Ptz};
use client::{value, Client};

pub struct Canon {
    client: Client,
    ranges: Ranges,
    last: Option<String>,
}

impl Canon {
    pub fn new(host: &str, user: &str, password: &str, ranges: Ranges, timeout: Duration) -> Canon {
        Canon { client: Client::new(host, user, password, timeout), ranges, last: None }
    }

    /// One control request. A `==?` in the echo means the camera rejected a
    /// value in its current mode.
    fn control(&mut self, query: &str) -> Result<()> {
        let pairs = self.client.get(&format!("control.cgi?{query}"))?;
        if let Some((name, _)) = pairs.iter().find(|(_, v)| v == "?") {
            return Err(Error::new(Code::Refused, format!("the camera rejected {name} in its current mode. Focus near and far need manual focus; exposure values need the matching exposure mode.")));
        }
        Ok(())
    }

    fn axis(v: f32, max: i32, plus: &str, minus: &str, name: &str) -> String {
        if v.abs() < DEAD {
            return format!("{name}=stop");
        }
        let speed = ((v.abs() * max as f32).round() as i32).max(if name == "zoom" { 0 } else { 10 });
        format!("{name}={}&{name}.speed.dir={speed}", if v > 0.0 { plus } else { minus })
    }

    fn query(&self, v: Velocity) -> String {
        let r = &self.ranges;
        [Self::axis(v.pan, r.pan_speed, "right", "left", "pan"), Self::axis(v.tilt, r.tilt_speed, "up", "down", "tilt"), Self::axis(v.zoom, r.zoom_speed, "tele", "wide", "zoom")].join("&")
    }
}

impl Ptz for Canon {
    fn protocol(&self) -> &'static str {
        "canon"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { absolute: true, position: true, focus: true, exposure: true, presets: 100 }
    }

    fn move_at(&mut self, v: Velocity) -> Result<()> {
        let q = self.query(v);
        if self.last.as_deref() == Some(q.as_str()) {
            return Ok(());
        }
        self.last = None;
        self.control(&q)?;
        self.last = Some(q);
        Ok(())
    }

    fn zoom(&mut self, speed: f32) -> Result<()> {
        self.last = None;
        self.control(&Self::axis(speed, self.ranges.zoom_speed, "tele", "wide", "zoom"))
    }

    fn focus(&mut self, f: Focus) -> Result<()> {
        let q = match f {
            Focus::Auto => "focus=auto".to_string(),
            Focus::Manual => "focus=manual".to_string(),
            Focus::OnePush => "c.1.focus.action=one_shot".to_string(),
            Focus::Drive(s) if s.abs() < DEAD => "focus.action=stop".to_string(),
            Focus::Drive(s) => format!("focus.action={}&focus.speed={}", if s > 0.0 { "far" } else { "near" }, ((s.abs() * 3.0).ceil() as i32 - 1).clamp(0, 2)),
        };
        self.control(&q)
    }

    fn goto_preset(&mut self, n: u16) -> Result<()> {
        check_preset(n, &self.capabilities())?;
        self.last = None;
        self.control(&format!("p={n}"))
    }

    fn save_preset(&mut self, n: u16) -> Result<()> {
        check_preset(n, &self.capabilities())?;
        let q = format!("preset/set?p={n}&name=preset-{n}&ptz=enabled&focus=enabled&exp=disabled&wb=disabled&is=disabled&cp=disabled");
        self.client.get(&q).map(|_| ())
    }

    fn position(&mut self) -> Result<Position> {
        let pairs = self.client.get("info.cgi?item=c.1.pan,c.1.tilt,c.1.zoom")?;
        let num = |n: &str| value(&pairs, n).and_then(|v| v.parse::<i32>().ok());
        let (Some(pan), Some(tilt), Some(zoom)) = (num("c.1.pan"), num("c.1.tilt"), num("c.1.zoom")) else {
            return Err(Error::new(Code::BadReply, "info.cgi gave no c.1.pan, c.1.tilt and c.1.zoom. The camera may be in standby or its firmware may be old."));
        };
        let (lo, hi) = self.ranges.zoom;
        let z = ((hi - zoom) as f32 / (hi - lo).max(1) as f32).clamp(0.0, 1.0);
        Ok(Position { pan: norm(pan, self.ranges.pan).clamp(-1.0, 1.0), tilt: norm(tilt, self.ranges.tilt).clamp(-1.0, 1.0), zoom: z, raw: [pan, tilt, zoom] })
    }

    fn goto(&mut self, p: Position) -> Result<()> {
        let r = self.ranges;
        let zoom = r.zoom.1 - (p.zoom.clamp(0.0, 1.0) * (r.zoom.1 - r.zoom.0) as f32).round() as i32;
        self.last = None;
        self.control(&format!("pan={}&tilt={}&zoom={zoom}", denorm(p.pan.clamp(-1.0, 1.0), r.pan), denorm(p.tilt.clamp(-1.0, 1.0), r.tilt)))
    }

    fn exposure(&mut self, e: Exposure) -> Result<()> {
        let mut q = Vec::new();
        if let Some(m) = e.mode {
            q.push(format!("c.1.exp={}", match m { ExposureMode::Auto => "auto", ExposureMode::Manual => "manual", ExposureMode::ShutterPriority => "tv", ExposureMode::IrisPriority => "av" }));
        }
        let parts = [(e.shutter, "c.1.me.shutter", "speed"), (e.iris, "c.1.me.diaphragm", "manual"), (e.gain, "c.1.me.gain", "manual")];
        for (v, key, mode) in parts {
            if let Some(v) = v {
                q.push(format!("{key}.mode={mode}&{key}={v}"));
            }
        }
        if q.is_empty() {
            return Ok(());
        }
        self.control(&q.join("&"))
    }
}
