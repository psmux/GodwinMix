//! The bus name a source's params make.
//!
//! `camera:<device id>` for the camera plugin, so the name reads as what it
//! is. Any other plugin's device is `camera:<plugin>-<id>`, because two
//! plugins may number their devices the same way and mean different things
//! (the camera plugin's device `0` is not the screen plugin's monitor `0`). A
//! value that is not already a short slug is made into one, with a hash of
//! the whole value on the end so that two values that slug the same stay
//! apart. A channel is `channel:<channel>/<stream>` exactly.

use anyhow::{bail, Result};
use godwinmix_framebus::BusName;
use godwinmix_protocol::plugin::share::Share;
use serde_json::Value;

/// The longest id kept as it is. Longer ones are cut and hashed, so the
/// socket path stays well inside the 103 bytes macOS allows.
const MAX_PLAIN: usize = 48;
const MAX_PREFIX: usize = 40;

/// The name `share` makes from `params`, or `None` when they name nothing to
/// share: a channel source with no stream is a listener of its own.
pub fn bus_name(plugin: &str, share: &Share, params: &Value) -> Result<Option<BusName>> {
    let values = share.values(params);
    match share.bus.as_str() {
        "channel" => channel(&values),
        "camera" => camera(plugin, &values).map(Some),
        other => bail!("the manifest shares on '{other}', which is neither camera nor channel"),
    }
}

fn channel(values: &[String]) -> Result<Option<BusName>> {
    let Some(value) = values.first().filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let Some((app, stream)) = value.split_once('/') else {
        bail!("'{value}' is not <channel>/<stream>");
    };
    Ok(Some(BusName::channel(app, stream)?))
}

fn camera(plugin: &str, values: &[String]) -> Result<BusName> {
    let joined = if values.iter().all(String::is_empty) {
        "default".to_string()
    } else {
        values.join("|")
    };
    let whole = if plugin == "camera" { joined } else { format!("{plugin}-{joined}") };
    Ok(BusName::camera(&slug(&whole))?)
}

/// `whole` itself when it is a slug short enough, otherwise a readable slug
/// of it and eight hex digits of its hash.
pub(super) fn slug(whole: &str) -> String {
    let plain = whole.len() <= MAX_PLAIN
        && whole.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if plain {
        return whole.to_string();
    }
    let mut out = String::new();
    for c in whole.chars() {
        let keep = c.is_ascii_alphanumeric() || c == '_';
        let next = if keep { c.to_ascii_lowercase() } else { '-' };
        if next == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(next);
        if out.len() >= MAX_PREFIX {
            break;
        }
    }
    let prefix = out.trim_end_matches('-');
    format!("{prefix}-{:08x}", fnv1a(whole.as_bytes()) as u32)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn camera_share() -> Share {
        Share { bus: "camera".into(), params: vec!["device".into()], ..Share::default() }
    }

    fn name(plugin: &str, share: &Share, params: Value) -> String {
        bus_name(plugin, share, &params).unwrap().unwrap().to_string()
    }

    #[test]
    fn a_camera_id_that_is_a_slug_reads_as_itself() {
        let id = "6C707041-05AC-0010-0008-000000000001";
        assert_eq!(name("camera", &camera_share(), json!({"device": id})), format!("camera:{id}"));
        assert_eq!(name("camera", &camera_share(), json!({})), "camera:default");
        assert_eq!(name("camera", &camera_share(), json!({"device": ""})), "camera:default");
    }

    #[test]
    fn another_plugin_is_kept_apart_and_a_path_is_hashed() {
        let a = name("screen", &camera_share(), json!({"device": "0"}));
        assert_eq!(a, "camera:screen-0");
        let v4l = name("camera", &camera_share(), json!({"device": "/dev/video0"}));
        assert!(v4l.starts_with("camera:dev-video0-"), "{v4l}");
        let other = name("camera", &camera_share(), json!({"device": "/dev/video0 "}));
        assert_eq!(v4l, other, "a value is trimmed before it is named");
        let long = name("camera", &camera_share(), json!({"device": "x".repeat(200)}));
        assert!(long.len() < "camera:".len() + 64, "{long}");
    }

    #[test]
    fn several_params_make_one_name_and_differ_when_any_differs() {
        let share = Share {
            bus: "camera".into(),
            params: vec!["monitor".into(), "region".into()],
            ..Share::default()
        };
        let whole = name("screen", &share, json!({"monitor": 1}));
        let part = name("screen", &share, json!({"monitor": 1, "region": "0,0,640,360"}));
        assert_ne!(whole, part);
    }

    #[test]
    fn a_channel_needs_a_stream_and_a_listener_is_not_shared() {
        let share = Share { bus: "channel".into(), params: vec!["stream".into()], ..Share::default() };
        let got = bus_name("ingest", &share, &json!({"stream": "sunday/main_720p"})).unwrap();
        assert_eq!(got.unwrap().to_string(), "channel:sunday/main_720p");
        assert!(bus_name("ingest", &share, &json!({"stream": ""})).unwrap().is_none());
        assert!(bus_name("ingest", &share, &json!({"stream": "nochannel"})).is_err());
    }
}
