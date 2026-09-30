//! Speed presets: which ones were measured, which fits a budget, which is
//! the next faster. Software encoders only; hardware has none here.

use super::Profile;
use godwinmix_protocol::rendition::{EncoderSlot, VideoShape};

impl Profile {
    /// The speed presets measured for a software encoder, fastest first.
    pub fn presets(&self, slot: &EncoderSlot) -> Vec<String> {
        self.enc(slot).map(|e| e.presets.iter().map(|(n, _)| n.clone()).collect()).unwrap_or_default()
    }

    /// The preset the catalogue configures for this encoder.
    pub fn configured_preset(&self, slot: &EncoderSlot) -> Option<String> {
        self.enc(slot).and_then(|e| e.cal.preset.clone())
    }

    /// The slowest (best looking) preset whose cost fits in `millicores`.
    pub fn preset_that_fits(&self, slot: &EncoderSlot, shape: &VideoShape, millicores: u32) -> Option<String> {
        self.presets(slot)
            .into_iter()
            .rev()
            .find(|p| self.encode_cost_at(slot, shape, Some(p)).cpu_millicores <= millicores)
    }

    /// The next faster preset than `current`, if one was measured.
    pub fn faster_preset(&self, slot: &EncoderSlot, current: Option<&str>) -> Option<String> {
        let names = self.presets(slot);
        let cur = current.map(str::to_string).or_else(|| self.configured_preset(slot))?;
        let at = names.iter().position(|n| *n == cur)?;
        at.checked_sub(1).map(|i| names[i].clone())
    }
}
