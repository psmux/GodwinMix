//! `list_screens`, the one tool this plugin contributes.
//!
//! It answers three things an operator or an agent needs before adding a
//! screen source: which element this machine would use, what the platform will
//! name, and what permission or prompt stands in the way. The third is the one
//! that matters: on macOS and on Wayland, a screen capture that nobody has
//! agreed to is a black picture and no error.

use godwinmix_capture_common::devices;
use godwinmix_sdk::prelude::*;
use godwinmix_sdk::wire::{ToolCall, ToolResult};
use serde_json::{json, Value};

use crate::pipeline;
use crate::settings::Settings;

/// The device classes a monitor might be listed under, where the platform
/// lists monitors at all.
pub const MONITORS: &[&str] = &["Source/Monitor", "Video/Source/Monitor"];

pub fn dispatch(method: &str, params: Value) -> Result<Value, RpcError> {
    if method != "tool.call" {
        return Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!(
                "this plugin has no method '{method}'. It answers the source methods and \
                 `tool.call` with name 'list_screens'."
            ),
        )
        .with_data(json!({"method": method, "retryable": false})));
    }
    let call: ToolCall = serde_json::from_value(params).map_err(|e| {
        RpcError::new(
            codes::INVALID_PARAMS,
            format!("`tool.call` needs {{name, arguments}}: {e}"),
        )
    })?;
    let result = call_tool(&call.name, call.arguments)?;
    serde_json::to_value(result)
        .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, format!("could not encode: {e}")))
}

pub fn call_tool(name: &str, _arguments: Value) -> Result<ToolResult, RpcError> {
    if name != "list_screens" {
        return Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!("this plugin has no tool '{name}'. It has one: 'list_screens'."),
        ));
    }
    let answer = report();
    Ok(ToolResult {
        content: json!([{"type": "text", "text": summary(&answer)}]),
        structured_content: Some(answer),
        is_error: Some(false),
    })
}

/// What this machine will capture, as the output schema promises.
pub fn report() -> Value {
    let element = pipeline::factory_for(&Settings::default()).unwrap_or_default();
    json!({
        "element": element,
        "screens": screens(),
        "note": note(&element),
    })
}

/// One entry per monitor the platform will name.
///
/// Most will not. Only Windows ships a GStreamer device provider for screens;
/// AVFoundation and X11 offer none, so the honest answer there is one entry
/// standing for the whole screen, and the `monitor` setting counts upwards
/// from it.
pub fn screens() -> Vec<Value> {
    let found = devices::list(MONITORS).unwrap_or_default();
    if found.is_empty() {
        return vec![json!({"index": 0, "name": "The whole screen"})];
    }
    found
        .iter()
        .enumerate()
        .map(|(index, d)| json!({"index": index, "name": d.name}))
        .collect()
}

/// What an operator has to do on this platform before a capture works.
fn note(element: &str) -> &'static str {
    match element {
        "" => {
            "this machine has no screen capture element. On Linux install \
             gstreamer1.0-plugins-good for ximagesrc, or gstreamer1.0-pipewire for Wayland."
        }
        "avfvideosrc" => {
            "macOS asks for screen recording permission the first time, and it asks the \
             program that started the mixer, which may be your terminal. Until someone \
             answers, the picture is black with no error. Grant it in System Settings, \
             Privacy and Security, Screen Recording, then restart the mixer."
        }
        "ximagesrc" => {
            "X11 needs no permission. On Wayland this captures nothing: set `node_id` from a \
             portal session instead, and see the plugin's README."
        }
        "pipewiresrc" => {
            "the desktop portal has already granted a node, so this will work until the \
             session ends. A new session needs a new node id."
        }
        _ => {
            "Windows needs no permission for desktop duplication. Exclusive fullscreen \
              games are not captured; ask the player to use borderless windowed mode."
        }
    }
}

fn summary(answer: &Value) -> String {
    let element = answer["element"].as_str().unwrap_or("");
    let count = answer["screens"].as_array().map(Vec::len).unwrap_or(0);
    if element.is_empty() {
        return format!(
            "no screen capture on this machine. {}",
            answer["note"].as_str().unwrap_or("")
        );
    }
    format!(
        "{count} screen(s) through {element}. {}",
        answer["note"].as_str().unwrap_or("")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_method_names_the_one_it_has() {
        let err = dispatch("teleport", json!({})).expect_err("no such method");
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
        assert!(err.message.contains("list_screens"), "{}", err.message);
    }

    #[test]
    fn there_is_always_at_least_one_screen_to_name() {
        godwinmix_capture_common::init().unwrap();
        let screens = screens();
        assert!(
            !screens.is_empty(),
            "a machine with a screen capture element has a screen"
        );
        assert!(screens[0]["index"].is_number());
        assert!(screens[0]["name"].is_string());
    }

    #[test]
    fn the_answer_carries_the_permission_an_operator_has_to_grant() {
        godwinmix_capture_common::init().unwrap();
        let answer = report();
        assert!(answer["note"].is_string());
        assert!(answer["screens"].is_array());
        if cfg!(target_os = "macos") {
            assert!(
                answer["note"]
                    .as_str()
                    .unwrap()
                    .contains("Screen Recording"),
                "macOS must say where the permission lives: {}",
                answer["note"]
            );
        }
    }

    #[test]
    fn a_machine_with_no_capture_element_says_what_to_install() {
        assert!(note("").contains("install"));
    }
}
