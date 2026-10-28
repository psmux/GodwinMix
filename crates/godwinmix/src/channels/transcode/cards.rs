//! The NVIDIA cards a conversion may run on, and each card's elements.
//!
//! GStreamer's NVIDIA plugin registers its decoders and encoders once per
//! card: `nvh264dec` and `nvh264enc` on CUDA device 0, `nvh264device1dec`
//! and `nvh264device1enc` on device 1, and so on. Each knows its device and
//! cannot be moved. The CUDA converters (`cudaconvertscale`, `cudadownload`)
//! are one element with a `cuda-device-id` to set. So a conversion is put on
//! a card by naming that card's decoder and encoder and setting the
//! converters' device, which the listener does from the node's `gpu`.
//!
//! A card is offered only when it has every NVIDIA decoder and encoder the
//! first card has, so a channel put on it never needs an element of another
//! card and its pictures never cross from one card's memory to another's.

use std::collections::BTreeMap;

use godwinmix_core::catalogue::select::Registry;

/// The most cards looked for.
const MOST: u32 = 16;

/// The catalogue's acceleration for NVIDIA, and the governor's device name
/// for the first card.
pub const NVIDIA: &str = "nvidia";

#[derive(Debug, Clone, Default)]
pub struct Cards {
    /// CUDA device indexes, in order. Empty with no NVIDIA card; `[0]` with one.
    pub list: Vec<u32>,
    /// The element on a card, by the first card's name and the device.
    names: BTreeMap<(String, u32), String>,
}

impl Cards {
    /// The cards of this machine that carry every one of `wanted`, the
    /// NVIDIA decoders and encoders the first card has.
    pub fn probe(mut wanted: Vec<String>, reg: &dyn Registry) -> Cards {
        wanted.sort();
        wanted.dedup();
        let mut cards = Cards::default();
        if wanted.is_empty() {
            return cards;
        }
        for device in 0..MOST {
            let found: Vec<(String, String)> = wanted.iter().filter_map(|w| reg.on_device(w, device).map(|n| (w.clone(), n))).collect();
            if found.len() < wanted.len() {
                continue;
            }
            cards.list.push(device);
            for (base, name) in found {
                cards.names.insert((base, device), name);
            }
        }
        cards
    }

    /// More than one card, so every conversion on one says which.
    pub fn several(&self) -> bool {
        self.list.len() > 1
    }

    pub fn has(&self, device: u32) -> bool {
        self.list.contains(&device)
    }

    /// `element` (the first card's name) as it is on `device`.
    pub fn element(&self, element: &str, device: Option<u32>) -> String {
        match device {
            Some(d) if d > 0 => self.names.get(&(element.to_string(), d)).cloned().unwrap_or_else(|| element.to_string()),
            _ => element.to_string(),
        }
    }
}

/// The governor's name for a card: `nvidia` for the first, which is what
/// calibrations have always called it, and `nvidia:1` and on after.
pub fn device_name(device: u32) -> String {
    match device {
        0 => NVIDIA.to_string(),
        n => format!("{NVIDIA}:{n}"),
    }
}

/// The card a governor device name is, for an NVIDIA one.
pub fn device_index(name: &str) -> Option<u32> {
    match name.split_once(':') {
        None if name == NVIDIA => Some(0),
        Some((NVIDIA, n)) => n.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_core::catalogue::select::FakeRegistry;

    fn cards(names: &[&str]) -> Cards {
        let wanted = ["nvh264enc", "nvh264dec", "nvmpeg2videodec"].iter().filter(|w| names.contains(w)).map(|w| w.to_string()).collect();
        Cards::probe(wanted, &FakeRegistry::with(names))
    }

    #[test]
    fn a_second_card_is_offered_with_its_own_elements() {
        let two = cards(&["nvh264enc", "nvh264dec", "nvmpeg2videodec", "nvh264device1enc", "nvh264device1dec", "nvmpeg2videodevice1dec"]);
        assert_eq!(two.list, vec![0, 1]);
        assert!(two.several());
        assert_eq!(two.element("nvh264dec", Some(1)), "nvh264device1dec");
        assert_eq!(two.element("nvmpeg2videodec", Some(1)), "nvmpeg2videodevice1dec");
        assert_eq!(two.element("nvh264enc", Some(1)), "nvh264device1enc");
        assert_eq!(two.element("nvh264enc", Some(0)), "nvh264enc");
        assert_eq!(two.element("nvh264enc", None), "nvh264enc");
    }

    #[test]
    fn a_card_without_every_element_of_the_first_is_not_offered() {
        let lame = cards(&["nvh264enc", "nvh264dec", "nvmpeg2videodec", "nvh264device1enc", "nvh264device1dec"]);
        assert_eq!(lame.list, vec![0]);
        assert!(!lame.several());
        assert!(cards(&["x264enc", "avdec_h264"]).list.is_empty());
    }

    #[test]
    fn device_names_keep_the_first_card_as_it_was() {
        assert_eq!(device_name(0), "nvidia");
        assert_eq!(device_name(1), "nvidia:1");
        assert_eq!(device_index("nvidia"), Some(0));
        assert_eq!(device_index("nvidia:3"), Some(3));
        assert_eq!(device_index("va"), None);
    }
}
