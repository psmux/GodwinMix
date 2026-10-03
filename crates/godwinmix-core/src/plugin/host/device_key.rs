//! What makes two sources the same device, on every platform.
//!
//! The frame bus (`shared`, Unix only) shares a camera between sources by the
//! manifest's `share` params. Where there is no bus, which is Windows today,
//! the same identity says when a second source would open a camera that one
//! already has: the platform refuses that, and the mixer waited five seconds
//! of its own thread to find out. `source.add` asks this first.

use crate::config::Params;
use serde_json::Value;

/// The `share` params' values for a provide that shares a camera, joined and
/// lower cased. `None` for anything that does not open a device (a channel,
/// a stream, a file) or a plugin that is not installed.
pub fn device_key(type_id: &str, params: &Value) -> Option<String> {
    let (plugin, provide) = type_id.split_once('/')?;
    let installed = crate::plugin::loader::get(plugin)?;
    let share = installed.manifest.provide(provide)?.share.clone()?;
    if share.bus != "camera" {
        return None;
    }
    // Empty is the platform's default camera, and two sources asking for it
    // are asking for the same one.
    Some(share.values(params).join("|").to_lowercase())
}

/// A source's params as the JSON a manifest's `share` reads.
pub fn params_value(params: &Params) -> Value {
    serde_json::to_value(params).unwrap_or(Value::Null)
}

/// Whether a second source of one device can read the first one's frames
/// here, which makes opening it twice a non question.
pub fn shared_on_this_machine() -> bool {
    #[cfg(unix)]
    {
        super::shared::enabled()
    }
    #[cfg(not(unix))]
    {
        false
    }
}
