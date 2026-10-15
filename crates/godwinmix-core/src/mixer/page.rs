//! A web page source driven by hand, on the mixer thread.
//!
//! All this thread does is find the source and hand its kind a list of lines,
//! which the kind queues for the renderer's own writer thread. It asks the
//! kind with `try_call`: a kind in the middle of a restart holds its lock for
//! as long as that takes, and this thread answers everybody else in the
//! meantime, so a busy page is refused with "try again" rather than waited on.
//!
//! A new address is the one thing kept: it goes into the source's config, so
//! the runtime store and a saved project open the page where the operator
//! left it.

use super::Mixer;
use crate::state::SourceId;
use anyhow::Result;
use serde_json::{json, Value};

/// What a page is asked to do.
#[derive(Debug, Clone)]
pub enum PageCall {
    /// Control lines, already checked: pointer, keys, back, forward, reload.
    Lines(Vec<String>),
    /// Go to this address and keep it.
    Go(String),
}

impl Mixer {
    pub(super) fn page(&mut self, id: &SourceId, call: PageCall) -> Result<Value> {
        let Some(slot) = self.sources.iter().find(|s| &s.input.id == id) else {
            anyhow::bail!("there is no source '{id}' to drive. Call source.list to see the ids");
        };
        let (method, params, going) = match call {
            PageCall::Lines(lines) => ("page", json!({ "lines": lines }), false),
            PageCall::Go(url) => ("page.go", json!({ "url": url }), true),
        };
        let answer = slot.input.try_call(method, params).unwrap_or_else(|| {
            Err(anyhow::anyhow!(
                "'{id}' is starting again and cannot take events this moment; send them again in a second"
            ))
        })?;
        if going {
            // `current_config` reads the new address back from the kind.
            self.persist_runtime();
            self.broadcast_status();
        }
        Ok(answer)
    }
}
