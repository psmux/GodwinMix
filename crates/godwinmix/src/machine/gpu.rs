//! Which way the GPUs are read, picked once when the sampler starts, and
//! the history each one keeps.

use super::{smi, KEEP};
use godwinmix_protocol::machine::{GpuDevice, GpuSample};

/// One GPU as a reader sees it this second.
pub struct Read {
    pub index: u32,
    pub name: String,
    pub driver: Option<String>,
    pub sample: GpuSample,
}

pub enum Reader {
    #[cfg(target_os = "linux")]
    Nvml(super::nvml::Nvml),
    Smi,
    None(String),
}

impl Reader {
    /// NVML if the driver's library opens, else `nvidia-smi` if it answers,
    /// else nothing, with why.
    pub fn open() -> Reader {
        #[cfg(target_os = "linux")]
        if let Some(n) = super::nvml::Nvml::open() {
            return Reader::Nvml(n);
        }
        match smi::query() {
            Ok(_) => Reader::Smi,
            Err(why) => Reader::None(why),
        }
    }

    pub fn source(&self) -> &'static str {
        match self {
            #[cfg(target_os = "linux")]
            Reader::Nvml(_) => "nvml",
            Reader::Smi => "nvidia-smi",
            Reader::None(_) => "none",
        }
    }

    pub fn note(&self) -> Option<String> {
        match self {
            Reader::None(why) => Some(why.clone()),
            _ => None,
        }
    }

    /// This second's readings. A reader that fails says nothing this time
    /// and is asked again next second.
    pub fn read(&mut self) -> Vec<Read> {
        match self {
            #[cfg(target_os = "linux")]
            Reader::Nvml(n) => n.read(),
            Reader::Smi => smi::query().unwrap_or_default(),
            Reader::None(_) => Vec::new(),
        }
    }
}

/// Put this second's readings into the devices, each keeping its last
/// [`KEEP`]. A GPU that stopped answering keeps its last reading.
pub fn keep(devices: &mut Vec<GpuDevice>, reads: Vec<Read>) {
    for r in reads {
        let at = match devices.iter().position(|d| d.index == r.index) {
            Some(at) => at,
            None => {
                devices.push(GpuDevice { index: r.index, ..Default::default() });
                devices.len() - 1
            }
        };
        let d = &mut devices[at];
        d.name = r.name;
        d.driver = r.driver.or(d.driver.take());
        d.history.push(r.sample.clone());
        if d.history.len() > KEEP {
            d.history.remove(0);
        }
        d.now = r.sample;
    }
    devices.sort_by_key(|d| d.index);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_keeps_two_minutes_and_its_newest_reading() {
        let mut devices = Vec::new();
        for at_ms in 0..(KEEP as u64 + 5) {
            let sample = GpuSample { at_ms, encoder_pct: Some(at_ms as u32 % 100), ..Default::default() };
            keep(&mut devices, vec![Read { index: 0, name: "RTX".into(), driver: None, sample }]);
        }
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].history.len(), KEEP);
        assert_eq!(devices[0].history[0].at_ms, 5);
        assert_eq!(devices[0].now.at_ms, KEEP as u64 + 4);
    }
}
