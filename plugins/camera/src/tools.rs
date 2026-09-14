//! `list_cameras`, the one tool this plugin contributes.
//!
//! Both provides answer it. A tool is declared per plugin rather than per
//! provide, and an agent asking what cameras exist should get the same answer
//! whether the process it reached happens to be a running source or the
//! discovery singleton.

use godwinmix_capture_common::devices;
use godwinmix_sdk::prelude::*;
use godwinmix_sdk::wire::{ToolCall, ToolResult};
use serde_json::{json, Value};

/// Route `tool.call`, and refuse anything else the way the SDK does.
pub fn dispatch(method: &str, params: Value) -> Result<Value, RpcError> {
    if method != "tool.call" {
        return Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!(
                "this plugin has no method '{method}'. It answers the source methods and \
                 `tool.call` with name 'list_cameras'."
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

/// The tool itself, in MCP's shape.
pub fn call_tool(name: &str, _arguments: Value) -> Result<ToolResult, RpcError> {
    if name != "list_cameras" {
        return Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!("this plugin has no tool '{name}'. It has one: 'list_cameras'."),
        ));
    }
    let listing = cameras().map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
    Ok(ToolResult {
        content: json!([{"type": "text", "text": summary(&listing)}]),
        structured_content: Some(json!({"cameras": listing})),
        is_error: Some(false),
    })
}

/// Every camera, in the shape `schemas/tools/list_cameras.out.json` promises.
pub fn cameras() -> Result<Vec<Value>, String> {
    Ok(devices::list(devices::CAMERA)?
        .into_iter()
        .map(|d| {
            let mut entry = json!({"id": d.id, "name": d.name});
            if let Some(api) = d.api {
                entry["api"] = json!(api);
            }
            if let Some((w, h)) = d.best_size {
                entry["width"] = json!(w);
                entry["height"] = json!(h);
            }
            entry
        })
        .collect())
}

/// The line a person reads when the structured answer is not what they wanted.
fn summary(cameras: &[Value]) -> String {
    if cameras.is_empty() {
        return "no cameras. Plug one in, check nothing else has it open, and on macOS and \
                Windows check the mixer has been granted camera permission."
            .into();
    }
    let names: Vec<String> = cameras
        .iter()
        .map(|c| {
            format!(
                "{} ({})",
                c["name"].as_str().unwrap_or("?"),
                c["id"].as_str().unwrap_or("?")
            )
        })
        .collect();
    format!("{} camera(s): {}", cameras.len(), names.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_method_names_the_one_it_has() {
        let err = dispatch("teleport", json!({})).expect_err("no such method");
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
        assert!(err.message.contains("list_cameras"), "{}", err.message);
    }

    #[test]
    fn an_unknown_tool_names_the_one_it_has() {
        let err = call_tool("list_microphones", json!({})).expect_err("no such tool");
        assert!(err.message.contains("list_cameras"), "{}", err.message);
    }

    #[test]
    fn a_bad_tool_call_is_minus_32602() {
        let err = dispatch("tool.call", json!({"nope": 1})).expect_err("no name");
        assert_eq!(err.code, codes::INVALID_PARAMS);
    }

    #[test]
    fn the_tool_answers_the_shape_the_output_schema_promises() {
        godwinmix_capture_common::init().unwrap();
        let result = call_tool("list_cameras", json!({})).expect("listing never fails");
        assert_eq!(result.is_error, Some(false));
        let structured = result.structured_content.expect("structured content");
        let cameras = structured["cameras"].as_array().expect("an array");
        for camera in cameras {
            assert!(camera["id"].is_string(), "{camera}");
            assert!(camera["name"].is_string(), "{camera}");
        }
    }

    #[test]
    fn an_empty_machine_is_told_what_to_check() {
        let text = summary(&[]);
        assert!(text.contains("permission"), "{text}");
    }
}
