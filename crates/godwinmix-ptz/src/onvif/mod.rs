//! ONVIF PTZ: ContinuousMove, AbsoluteMove, Stop, GetStatus and presets on
//! the PTZ service, focus and exposure on the imaging service.
//!
//! * [`soap`]: the envelope and the WS-Security login, shared with ipcam.
//! * [`xml`]: reading answers whatever the namespace prefixes.
//! * [`body`]: the bodies of each call and the readers of each answer.

pub mod body;
pub mod soap;
pub mod xml;

use std::time::Duration;

use crate::error::{Code, Error, Result};
use crate::types::{Capabilities, Exposure, ExposureMode, Focus, Position, Velocity};
use crate::{check_preset, Ptz};
pub use soap::Login;
use soap::checked as call;

pub struct Onvif {
    login: Login,
    timeout: Duration,
    ptz: String,
    imaging: Option<String>,
    profile: String,
    video: Option<String>,
    last: Option<Velocity>,
}

impl Onvif {
    /// Ask the device service at `device` where its PTZ and imaging services
    /// are, and pick `profile` (or the first profile).
    pub fn connect(device: &str, login: Login, profile: Option<&str>, timeout: Duration) -> Result<Onvif> {
        let caps = call(device, &login, body::capabilities(), timeout)?;
        let ptz = body::service(&caps, "PTZ").ok_or_else(|| Error::new(Code::Unsupported, format!("{device} has no PTZ service. It is a fixed camera, or its PTZ is switched off in its web page.")))?;
        let media = body::service(&caps, "Media").unwrap_or_else(|| device.to_string());
        let profiles = body::read_profiles(&call(&media, &login, body::profiles(), timeout)?);
        let chosen = match profile {
            Some(want) => profiles.iter().find(|(t, _)| t == want),
            None => profiles.first(),
        };
        let Some((token, video)) = chosen.cloned() else {
            let known: Vec<&str> = profiles.iter().map(|(t, _)| t.as_str()).collect();
            return Err(Error::new(Code::BadArgument, format!("{device} has no profile {:?}. Its profiles are: {}.", profile.unwrap_or("at all"), known.join(", "))));
        };
        Ok(Onvif { login, timeout, ptz, imaging: body::service(&caps, "Imaging"), profile: token, video, last: None })
    }

    fn ptz_call(&self, body: &str) -> Result<String> {
        call(&self.ptz, &self.login, body, self.timeout)
    }

    fn imaging_call(&self, what: &str, make: impl FnOnce(&str) -> String) -> Result<String> {
        match (&self.imaging, &self.video) {
            (Some(url), Some(video)) => call(url, &self.login, &make(video), self.timeout),
            _ => Err(Error::unsupported("onvif", what)),
        }
    }

    /// The preset token saved as GodwinMix preset `n`: one named `preset-n`,
    /// else one whose token or name is `n` itself.
    fn find_preset(&self, n: u16) -> Result<Option<String>> {
        let list = body::read_presets(&self.ptz_call(&body::presets(&self.profile))?);
        let (name, plain) = (preset_name(n), n.to_string());
        Ok(list.iter().find(|(_, nm)| *nm == name).or_else(|| list.iter().find(|(t, nm)| *t == plain || *nm == plain)).map(|(t, _)| t.clone()))
    }
}

pub fn preset_name(n: u16) -> String {
    format!("preset-{n}")
}

impl Ptz for Onvif {
    fn protocol(&self) -> &'static str {
        "onvif"
    }
    fn second(&self) -> Option<Box<dyn Ptz>> {
        let o = Onvif { login: self.login.clone(), ptz: self.ptz.clone(), imaging: self.imaging.clone(), profile: self.profile.clone(), video: self.video.clone(), last: None, ..*self };
        Some(Box::new(o))
    }

    fn capabilities(&self) -> Capabilities {
        let imaging = self.imaging.is_some() && self.video.is_some();
        Capabilities { absolute: true, position: true, focus: imaging, exposure: imaging, presets: 100 }
    }

    fn move_at(&mut self, v: Velocity) -> Result<()> {
        if self.last == Some(v) {
            return Ok(());
        }
        self.last = None;
        if v.is_stop() {
            self.ptz_call(&body::stop(&self.profile))?;
        } else {
            self.ptz_call(&body::continuous_move(&self.profile, v.pan, v.tilt, v.zoom))?;
        }
        self.last = Some(v);
        Ok(())
    }

    fn zoom(&mut self, speed: f32) -> Result<()> {
        let v = Velocity { zoom: speed, ..self.last.unwrap_or_default() };
        self.move_at(v)
    }

    fn focus(&mut self, f: Focus) -> Result<()> {
        match f {
            Focus::Auto | Focus::Manual => self.imaging_call("focus", |v| body::imaging(v, None, Some(f == Focus::Auto))),
            Focus::Drive(s) if s.abs() < crate::types::DEAD => self.imaging_call("focus", body::focus_stop),
            Focus::Drive(s) => self.imaging_call("focus", |v| body::focus_move(v, s)),
            Focus::OnePush => Err(Error::new(Code::Unsupported, "ONVIF has no one push focus. Switch focus to auto, then back to manual once it is sharp.")),
        }
        .map(|_| ())
    }

    fn goto_preset(&mut self, n: u16) -> Result<()> {
        check_preset(n, &self.capabilities())?;
        let token = self.find_preset(n)?.ok_or_else(|| Error::new(Code::BadArgument, format!("this camera has no preset {n}. Save one first with save_preset.")))?;
        self.last = None;
        self.ptz_call(&body::goto_preset(&self.profile, &token)).map(|_| ())
    }

    fn save_preset(&mut self, n: u16) -> Result<()> {
        check_preset(n, &self.capabilities())?;
        let existing = self.find_preset(n)?;
        self.ptz_call(&body::set_preset(&self.profile, &preset_name(n), existing.as_deref())).map(|_| ())
    }

    fn position(&mut self) -> Result<Position> {
        let answer = self.ptz_call(&body::status(&self.profile))?;
        let (pan, tilt, zoom) = body::read_status(&answer).ok_or_else(|| Error::new(Code::BadReply, "the camera's GetStatus answer has no position in it. It may not report one; presets still work."))?;
        let raw = [(pan * 1000.0) as i32, (tilt * 1000.0) as i32, (zoom * 1000.0) as i32];
        Ok(Position { pan, tilt, zoom, raw })
    }

    fn goto(&mut self, p: Position) -> Result<()> {
        self.last = None;
        self.ptz_call(&body::absolute_move(&self.profile, p.pan.clamp(-1.0, 1.0), p.tilt.clamp(-1.0, 1.0), p.zoom.clamp(0.0, 1.0))).map(|_| ())
    }

    fn exposure(&mut self, e: Exposure) -> Result<()> {
        if matches!(e.mode, Some(ExposureMode::ShutterPriority | ExposureMode::IrisPriority)) {
            return Err(Error::new(Code::Unsupported, "ONVIF exposure is auto or manual only. Use manual and set the shutter, or auto."));
        }
        self.imaging_call("exposure", |v| body::imaging(v, Some(&e), None)).map(|_| ())
    }
}
