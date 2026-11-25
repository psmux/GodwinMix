//! An answer given later, from any thread.
//!
//! The runtime runs one call at a time on its worker thread. A plugin whose
//! calls can be slow (a camera that takes a second to store a preset) and
//! whose other calls must not wait for them (a joystick that has to arrive
//! within 300 ms) takes the call with a [`Reply`], hands it to a thread of
//! its own and returns at once. The next call is then answered while the
//! slow one is still running.
//!
//! A `Reply` always answers: dropped without one, it answers with an error
//! that says so, so a caller is never left waiting on a lost call.

use std::sync::Arc;

use serde_json::Value;

use crate::framing::Writer;
use crate::wire::{codes, Id, RpcError, ToolResult};

pub struct Reply {
    writer: Arc<Writer>,
    id: Option<Id>,
    method: String,
    sent: bool,
}

impl Reply {
    pub(crate) fn new(writer: Arc<Writer>, id: Option<Id>, method: &str) -> Reply {
        Reply { writer, id, method: method.to_string(), sent: false }
    }

    /// A reply that writes to `writer`, for a test that drives a handler
    /// without the runtime.
    pub fn to(writer: Arc<Writer>, id: Id, method: &str) -> Reply {
        Reply::new(writer, Some(id), method)
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    /// Answer the call.
    pub fn send(mut self, outcome: Result<Value, RpcError>) {
        self.sent = true;
        write(&self.writer, self.id.take(), outcome);
    }

    /// Answer a `tool.call`.
    pub fn tool(self, outcome: Result<ToolResult, RpcError>) {
        let value = outcome.and_then(|r| {
            serde_json::to_value(r).map_err(|e| RpcError::new(codes::INTERNAL_ERROR, format!("could not encode the result: {e}")))
        });
        self.send(value);
    }
}

impl Drop for Reply {
    fn drop(&mut self) {
        if !self.sent {
            let why = format!("the plugin dropped the call '{}' without answering it. Retry; if it happens again, the plugin's log says why.", self.method);
            write(&self.writer, self.id.take(), Err(RpcError::new(codes::INTERNAL_ERROR, why)));
        }
    }
}

fn write(writer: &Writer, id: Option<Id>, outcome: Result<Value, RpcError>) {
    let Some(id) = id else {
        if let Err(e) = outcome {
            let _ = writer.log(crate::wire::LogLevel::Warn, &format!("notification failed: {e}"));
        }
        return;
    };
    let _ = match outcome {
        Ok(value) => writer.respond(id, value),
        Err(error) => writer.respond_error(Some(id), error),
    };
}
