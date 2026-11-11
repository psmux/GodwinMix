//! Reading the arguments, each refusal naming the argument and a good value.

use serde_json::Value;

use super::bad;
use crate::desk::Entry;
use godwinmix_sdk::prelude::RpcError;

/// `switcher`, which may be left out when there is one.
pub fn switcher(a: &Value) -> Option<&str> {
    a.get("switcher").or_else(|| a.get("id")).and_then(Value::as_str)
}

/// A small number counted from zero, `default` when left out.
pub fn index(a: &Value, key: &str, default: Option<u8>, example: &str) -> Result<u8, RpcError> {
    match a.get(key) {
        None | Some(Value::Null) => default.ok_or_else(|| bad(format!("give '{key}', counted from 0. Example: {example}"))),
        Some(v) => v
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .ok_or_else(|| bad(format!("'{key}' is {v}; give a whole number counted from 0. Example: {example}"))),
    }
}

/// The M/E, 0 when left out.
pub fn me(a: &Value) -> Result<u8, RpcError> {
    index(a, "me", Some(0), "{me: 0} for the first M/E")
}

pub fn flag(a: &Value, key: &str, example: &str) -> Result<bool, RpcError> {
    a.get(key).and_then(Value::as_bool).ok_or_else(|| bad(format!("give '{key}' as true or false. Example: {example}")))
}

pub fn text<'a>(a: &'a Value, key: &str, example: &str) -> Result<&'a str, RpcError> {
    a.get(key).and_then(Value::as_str).ok_or_else(|| bad(format!("give '{key}'. Example: {example}")))
}

/// An ATEM input: its number, or a GodwinMix source id the switcher's map
/// names.
pub fn input(a: &Value, key: &str, entry: &Entry) -> Result<u16, RpcError> {
    match a.get(key) {
        Some(Value::String(source)) => entry.map.get(source).copied().ok_or_else(|| {
            let mapped: Vec<&str> = entry.map.keys().map(String::as_str).collect();
            let mapped = if mapped.is_empty() { "nothing yet".to_string() } else { mapped.join(", ") };
            bad(format!("'{source}' is not mapped to an input on this switcher (mapped: {mapped}). Give the input number, or map it first with map {{source: \"{source}\", input: 1}}."))
        }),
        Some(v) => v
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .ok_or_else(|| bad(format!("'{key}' is {v}; give an input number such as 1, or 1000 for bars, as state lists them"))),
        None => Err(bad(format!("give '{key}', an input number as state lists them, such as {{{key}: 1}}"))),
    }
}
