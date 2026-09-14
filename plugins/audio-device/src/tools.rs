//! `list_audio_inputs`, the one tool this plugin contributes.

use godwinmix_capture_common::devices;
use godwinmix_sdk::prelude::*;
use godwinmix_sdk::wire::{ToolCall, ToolResult};
use serde_json::{json, Value};

pub fn dispatch(method: &str, params: Value) -> Result<Value, RpcError> {
    if method != "tool.call" {
        return Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!(
                "this plugin has no method '{method}'. It answers the source methods, \
                 `audio.set` and `tool.call` with name 'list_audio_inputs'."
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
    if name != "list_audio_inputs" {
        return Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!("this plugin has no tool '{name}'. It has one: 'list_audio_inputs'."),
        ));
    }
    let listing = inputs().map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
    Ok(ToolResult {
        content: json!([{"type": "text", "text": summary(&listing)}]),
        structured_content: Some(json!({"inputs": listing})),
        is_error: Some(false),
    })
}

/// Every input, in the shape the output schema promises.
pub fn inputs() -> Result<Vec<Value>, String> {
    Ok(devices::list(devices::MICROPHONE)?
        .into_iter()
        .map(|d| {
            let mut entry = json!({"id": d.id, "name": d.name});
            if let Some(api) = d.api {
                entry["api"] = json!(api);
            }
            entry
        })
        .collect())
}

fn summary(inputs: &[Value]) -> String {
    if inputs.is_empty() {
        return "no sound inputs. Check the device is plugged in and not held by another \
                program, and on macOS and Windows that the mixer has been granted microphone \
                permission."
            .into();
    }
    let names: Vec<String> = inputs
        .iter()
        .map(|i| {
            format!(
                "{} ({})",
                i["name"].as_str().unwrap_or("?"),
                i["id"].as_str().unwrap_or("?")
            )
        })
        .collect();
    format!("{} input(s): {}", inputs.len(), names.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_method_names_the_one_it_has() {
        let err = dispatch("teleport", json!({})).expect_err("no such method");
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
        assert!(err.message.contains("list_audio_inputs"), "{}", err.message);
    }

    #[test]
    fn an_unknown_tool_names_the_one_it_has() {
        let err = call_tool("list_cameras", json!({})).expect_err("no such tool");
        assert!(err.message.contains("list_audio_inputs"), "{}", err.message);
    }

    #[test]
    fn the_tool_answers_the_shape_the_output_schema_promises() {
        godwinmix_capture_common::init().unwrap();
        let result = call_tool("list_audio_inputs", json!({})).expect("listing never fails");
        let structured = result.structured_content.expect("structured content");
        for input in structured["inputs"].as_array().expect("an array") {
            assert!(input["id"].is_string(), "{input}");
            assert!(input["name"].is_string(), "{input}");
        }
    }

    #[test]
    fn an_empty_machine_is_told_what_to_check() {
        assert!(summary(&[]).contains("permission"));
    }
}
