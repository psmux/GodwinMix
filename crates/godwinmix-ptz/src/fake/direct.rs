//! [`FakePtz`]: a fake head driven directly, with no protocol in between.

use super::Shared;
use crate::error::Result;
use crate::types::{Capabilities, Exposure, Focus, Position, Velocity};
use crate::{check_preset, Ptz};

/// A camera with no protocol in between.
pub struct FakePtz(pub Shared);

impl Ptz for FakePtz {
    fn protocol(&self) -> &'static str {
        "fake"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { absolute: true, position: true, focus: true, exposure: true, presets: 100 }
    }
    fn move_at(&mut self, v: Velocity) -> Result<()> {
        self.0.lock().unwrap().set_velocity(v);
        Ok(())
    }
    fn zoom(&mut self, speed: f32) -> Result<()> {
        let mut h = self.0.lock().unwrap();
        let v = Velocity { zoom: speed, ..h.velocity };
        h.set_velocity(v);
        Ok(())
    }
    fn focus(&mut self, f: Focus) -> Result<()> {
        self.0.lock().unwrap().focus(f);
        Ok(())
    }
    fn goto_preset(&mut self, n: u16) -> Result<()> {
        check_preset(n, &self.capabilities())?;
        self.0.lock().unwrap().recall(n);
        Ok(())
    }
    fn save_preset(&mut self, n: u16) -> Result<()> {
        check_preset(n, &self.capabilities())?;
        let wait = self.0.lock().unwrap().save_delay;
        std::thread::sleep(wait);
        self.0.lock().unwrap().save(n);
        Ok(())
    }
    fn position(&mut self) -> Result<Position> {
        let wait = self.0.lock().unwrap().query_delay;
        std::thread::sleep(wait);
        Ok(self.0.lock().unwrap().position())
    }
    fn second(&self) -> Option<Box<dyn Ptz>> {
        Some(Box::new(FakePtz(self.0.clone())))
    }
    fn goto(&mut self, p: Position) -> Result<()> {
        self.0.lock().unwrap().jump(p.pan, p.tilt, p.zoom);
        Ok(())
    }
    fn exposure(&mut self, e: Exposure) -> Result<()> {
        let mut h = self.0.lock().unwrap();
        h.exposure = Exposure { mode: e.mode.or(h.exposure.mode), iris: e.iris.or(h.exposure.iris), shutter: e.shutter.or(h.exposure.shutter), gain: e.gain.or(h.exposure.gain) };
        Ok(())
    }
}
