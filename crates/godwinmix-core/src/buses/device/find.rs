//! Finding a sound output by name, id or number, with GStreamer's
//! `DeviceMonitor`.

use crate::gstutil::make;
use godwinmix_protocol::buses::AudioDevice;
use gstreamer as gst;
use gstreamer::prelude::*;

/// The properties that identify an output device, most stable first: the
/// same list the capture plugins use for inputs.
const ID_KEYS: &[&str] =
    &["device.path", "unique-id", "device.strid", "device.serial", "device.id", "object.path"];

/// Every sound output GStreamer can see here. Blocks for up to a few seconds
/// while the platform's providers start, so it is never called on the mixer
/// thread.
pub fn list() -> Vec<(AudioDevice, gst::Device)> {
    let monitor = gst::DeviceMonitor::new();
    monitor.add_filter(Some("Audio/Sink"), None);
    if monitor.start().is_err() {
        return Vec::new();
    }
    wait_until_started(&monitor);
    let devices = monitor.devices();
    monitor.stop();
    devices.into_iter().enumerate().map(|(index, d)| (describe(index, &d), d)).collect()
}

/// `GST_MESSAGE_DEVICE_MONITOR_STARTED`. See `capture-common` for why a 1.28
/// monitor has to be waited for before it is read.
const MONITOR_STARTED: u32 = 0x8000_0009;

fn wait_until_started(monitor: &gst::DeviceMonitor) {
    use gst::glib::translate::IntoGlib;
    if gst::version() < (1, 28, 0, 0) {
        return;
    }
    let bus = monitor.bus();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while let Some(left) = until.checked_duration_since(std::time::Instant::now()) {
        let Some(msg) = bus.timed_pop(gst::ClockTime::from_nseconds(left.as_nanos() as u64)) else { return };
        if msg.type_().into_glib() == MONITOR_STARTED {
            return;
        }
    }
}

fn describe(index: usize, device: &gst::Device) -> AudioDevice {
    let name = device.display_name().to_string();
    let props = device.properties();
    let value = |k: &str| props.as_ref().and_then(|s| s.get::<String>(k).ok()).filter(|v| !v.is_empty());
    AudioDevice {
        index,
        id: ID_KEYS.iter().find_map(|k| value(k)).unwrap_or_else(|| name.clone()),
        name,
        api: value("device.api"),
    }
}

/// The sink for `wanted`: `default`, or a device by id, name or number.
/// Answers with an error that lists what there is.
pub fn sink_for(wanted: &str) -> Result<gst::Element, String> {
    if wanted.is_empty() || wanted == "default" {
        return make("autoaudiosink", "bus-device-sink").map_err(|e| format!("{e:#}"));
    }
    let found = list();
    let pick = found
        .iter()
        .find(|(d, _)| d.id == wanted || d.name == wanted)
        .or_else(|| wanted.parse::<usize>().ok().and_then(|i| found.get(i)));
    match pick {
        Some((d, dev)) => dev.create_element(Some("bus-device-sink")).map_err(|e| format!("could not open {}: {e}", d.name)),
        None => {
            let names: Vec<String> = found.iter().map(|(d, _)| format!("{} ({})", d.name, d.index)).collect();
            Err(format!(
                "no sound output called '{wanted}' here. The outputs are: {}. Use a name or a number \
                 from audio.device.list, or `default`.",
                if names.is_empty() { "none".into() } else { names.join(", ") }
            ))
        }
    }
}
