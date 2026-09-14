//! Who is sending NDI on this network.
//!
//! NDI announces itself over mDNS as `_ndi._tcp`, and GStreamer's NDI plugin
//! already has a device provider that listens for it. Using that rather than
//! writing an mDNS client means one implementation of the discovery, the same
//! one the rest of the GStreamer world uses, and no second mDNS responder
//! fighting Avahi or Bonjour for the port.

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::{json, Value};

/// One sender seen on the network.
#[derive(Debug, Clone, PartialEq)]
pub struct Sender {
    /// The NDI name, which is what goes in a source's settings: `MACHINE (CAM 1)`.
    pub name: String,
    /// `host:port`, where the provider gives one.
    pub address: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    /// Frames per second, as a decimal, where the provider gives one.
    pub fps: Option<f64>,
}

impl Sender {
    /// The legible id a source gets when it is added for this sender.
    pub fn slug(&self) -> String {
        let mut out = String::with_capacity(self.name.len());
        let mut last_dash = true;
        for c in self.name.chars() {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_alphanumeric() {
                out.push(c);
                last_dash = false;
            } else if !last_dash {
                out.push('-');
                last_dash = true;
            }
        }
        let trimmed = out.trim_matches('-').to_string();
        if trimmed.is_empty() || !trimmed.starts_with(|c: char| c.is_ascii_alphabetic()) {
            format!("ndi-{trimmed}")
        } else {
            trimmed
        }
    }

    pub fn json(&self) -> Value {
        // `id` is the slug a source would sensibly be added under, so an agent
        // reading this does not have to invent one and does not reach for a
        // UUID. Ids in GodwinMix are slugs.
        let mut out = json!({ "name": self.name, "id": self.slug() });
        if !self.address.is_empty() {
            out["address"] = json!(self.address);
        }
        if let Some(v) = self.width {
            out["width"] = json!(v);
        }
        if let Some(v) = self.height {
            out["height"] = json!(v);
        }
        if let Some(v) = self.fps {
            out["fps"] = json!(v);
        }
        out
    }
}

/// Listen for `timeout_ms` and report every NDI sender seen.
///
/// mDNS answers arrive over a second or so, so a timeout under about 500 ms
/// will usually find nothing on a quiet network even when senders are there.
pub fn list(timeout_ms: u64) -> Result<Vec<Sender>, String> {
    gmx_netkit::init()?;
    if !gmx_netkit::elements::exists("ndisrc") {
        return Err(format!(
            "this build of GStreamer has no NDI plugin, so there is nothing to discover \
             with. It comes from {}.",
            gmx_netkit::elements::where_from("ndisrc")
        ));
    }
    let monitor = gst::DeviceMonitor::new();
    // Source/Network is the class the NDI device provider registers under. The
    // filter keeps a camera or a screen capture provider out of the answer.
    monitor.add_filter(Some("Source/Network"), None);
    monitor
        .start()
        .map_err(|e| format!("the device monitor would not start: {e}"))?;
    // The provider needs a moment to hear the announcements; there is no event
    // that says "that is all of them", because on mDNS there never is.
    std::thread::sleep(std::time::Duration::from_millis(timeout_ms.clamp(100, 10_000)));
    let devices = monitor.devices();
    monitor.stop();

    let mut out: Vec<Sender> = devices
        .iter()
        .filter(|device| is_ndi(device))
        .map(sender_of)
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    Ok(out)
}

/// Is this device one of the NDI provider's?
fn is_ndi(device: &gst::Device) -> bool {
    if device
        .properties()
        .and_then(|p| p.get::<String>("ndi-name").ok())
        .is_some()
    {
        return true;
    }
    device
        .caps()
        .map(|caps| {
            caps.iter()
                .any(|s| s.name().as_str().starts_with("application/x-ndi"))
        })
        .unwrap_or(false)
}

fn sender_of(device: &gst::Device) -> Sender {
    let properties = device.properties();
    let string = |key: &str| {
        properties
            .as_ref()
            .and_then(|p| p.get::<String>(key).ok())
            .unwrap_or_default()
    };
    let name = {
        let ndi = string("ndi-name");
        if ndi.is_empty() {
            device.display_name().to_string()
        } else {
            ndi
        }
    };
    let address = {
        let url = string("url-address");
        if url.is_empty() {
            string("ndi-url-address")
        } else {
            url
        }
    };
    let (mut width, mut height, mut fps) = (None, None, None);
    if let Some(caps) = device.caps() {
        if let Some(structure) = caps.structure(0) {
            width = structure.get::<i32>("width").ok();
            height = structure.get::<i32>("height").ok();
            fps = structure
                .get::<gst::Fraction>("framerate")
                .ok()
                .map(|f| f.numer() as f64 / f.denom().max(1) as f64)
                .map(|v| (v * 100.0).round() / 100.0);
        }
    }
    Sender { name, address, width, height, fps }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sender_name_becomes_a_legible_slug() {
        let sender = |name: &str| Sender {
            name: name.into(),
            address: String::new(),
            width: None,
            height: None,
            fps: None,
        };
        assert_eq!(sender("STUDIO (CAM 1)").slug(), "studio-cam-1");
        assert_eq!(sender("desk-mac").slug(), "desk-mac");
        assert_eq!(sender("4K RIG").slug(), "ndi-4k-rig");
        assert_eq!(sender("").slug(), "ndi-");
    }

    #[test]
    fn the_json_leaves_out_what_the_provider_did_not_say() {
        let bare = Sender {
            name: "CAM".into(),
            address: String::new(),
            width: None,
            height: None,
            fps: None,
        };
        let json = bare.json();
        assert_eq!(json["name"], "CAM");
        assert_eq!(json["id"], "cam");
        assert!(json.get("address").is_none());
        assert!(json.get("width").is_none());

        let full = Sender {
            name: "CAM".into(),
            address: "10.0.0.21:5961".into(),
            width: Some(1920),
            height: Some(1080),
            fps: Some(30.0),
        };
        assert_eq!(full.json()["width"], 1920);
        assert_eq!(full.json()["fps"], 30.0);
    }

    #[test]
    fn listing_on_a_machine_with_no_ndi_plugin_says_where_it_comes_from() {
        gmx_netkit::init().expect("gstreamer");
        match list(200) {
            // With the plugin present, an empty list on a quiet network is the
            // right answer and not an error.
            Ok(found) => assert!(found.len() < 1000),
            Err(why) => assert!(why.contains("NDI"), "{why}"),
        }
    }
}
