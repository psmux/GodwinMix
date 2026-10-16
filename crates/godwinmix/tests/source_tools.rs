//! `tool.call` with a `source`: a tool of a source plugin, answered by the
//! process behind that one source.
//!
//! A source plugin runs one process per source, so a playlist plugin with
//! three playlists has three processes and "skip" has to name which. These
//! check the routing and every refusal on a real core with two colour bar
//! sources. The answer from a plugin process itself is exercised by the
//! playout plugin's own live test (psmux/godwinmix-playout, dev/), which
//! needs a plugin installed.

use godwinmix::control::{call, methods, AppState};
use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::{self, Mixer};
use godwinmix_core::snapshot::Tracker;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::Token;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

#[path = "live/harness.rs"]
mod harness;
use harness::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_call_naming_a_source_goes_to_that_source_or_says_why_not() {
    let core = Core::start(godwinmix_core::safety::SafetyConfig::default()).await;
    let token = desk();

    let missing = core
        .call(&token, "tool.call", json!({ "name": "playout/state", "source": "nope", "arguments": {} }))
        .await
        .expect_err("there is no source called nope");
    assert_eq!(missing.code, ErrorCode::NotFound.number(), "{}", missing.message);
    assert!(missing.message.contains("cam1"), "names the sources that exist: {}", missing.message);

    let other = core
        .call(&token, "tool.call", json!({ "name": "playout/state", "source": "cam1", "arguments": {} }))
        .await
        .expect_err("cam1 is colour bars, not a playout source");
    assert_eq!(other.code, ErrorCode::InvalidParams.number(), "{}", other.message);
    assert!(other.message.contains("cam1"), "{}", other.message);
    assert!(other.data.get("plugin").is_some(), "names its plugin: {:?}", other.data);

    let bare = core
        .call(&token, "tool.call", json!({ "name": "state", "source": "cam1", "arguments": {} }))
        .await
        .expect_err("a built in source has no tools");
    assert_eq!(bare.code, ErrorCode::NotInState.number(), "{}", bare.message);
    assert!(bare.message.contains("did not answer"), "{}", bare.message);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_source_in_the_arguments_is_tried_only_when_no_service_has_the_tool() {
    let core = Core::start(godwinmix_core::safety::SafetyConfig::default()).await;
    let token = desk();
    // This is the shape an MCP client sends: the source among the tool's own
    // arguments. No service has `state` and nope is not a source, so the
    // refusal is the first one, which lists the tools that do exist.
    let e = core
        .call(&token, "tool.call", json!({ "name": "playout/state", "arguments": { "source": "nope" } }))
        .await
        .expect_err("nothing answers");
    assert_eq!(e.code, ErrorCode::NotFound.number(), "{}", e.message);
    assert!(e.message.contains("no tool called"), "{}", e.message);
}
