//! Asking the mixer what it is running, and stopping it, over the public API.
//!
//! Short timeouts, because both are asked while a person waits on a window
//! that is closing or a tray menu that is open.

use std::time::Duration;

use serde_json::Value;

use crate::core_link::Target;
use crate::running::{describe, Thing};

const ASK: Duration = Duration::from_secs(3);

/// What the mixer said it is running, or why it could not be asked.
#[derive(Debug, Clone, PartialEq)]
pub enum Known {
    Things(Vec<Thing>),
    /// It did not answer. It may well still be streaming.
    Unknown(String),
}

impl Known {
    pub fn lines(&self) -> Vec<String> {
        match self {
            Known::Things(things) => things.iter().map(|t| t.line.clone()).collect(),
            Known::Unknown(_) => Vec::new(),
        }
    }

    pub fn outgoing(&self) -> Vec<Thing> {
        match self {
            Known::Things(things) => things.iter().filter(|t| t.outgoing).cloned().collect(),
            Known::Unknown(_) => Vec::new(),
        }
    }
}

async fn get(http: &reqwest::Client, target: &Target, path: &str) -> Result<Value, String> {
    let mut req = http.get(format!("{}{path}", target.base)).timeout(ASK);
    if !target.token.is_empty() {
        req = req.bearer_auth(&target.token);
    }
    let reply = req.send().await.map_err(|e| e.to_string())?;
    if !reply.status().is_success() {
        return Err(format!("{path} answered {}", reply.status()));
    }
    reply.json().await.map_err(|e| e.to_string())
}

/// Read both lists. A mixer with no channels answers the outputs alone.
pub async fn read(http: &reqwest::Client, target: &Target) -> Known {
    let outputs = match get(http, target, "/api/v1/outputs").await {
        Ok(outputs) => outputs,
        Err(why) => return Known::Unknown(why),
    };
    let channels = get(http, target, "/api/v1/channels").await.unwrap_or(Value::Null);
    Known::Things(describe(&outputs, &channels))
}

/// Stop every outgoing thing. The errors, in words, for anything that would
/// not stop; empty when everything did.
pub async fn stop_all(http: &reqwest::Client, target: &Target, things: &[Thing]) -> Vec<String> {
    let mut failed = Vec::new();
    for thing in things.iter().filter(|t| t.outgoing) {
        let Some(stop) = &thing.stop else { continue };
        let url = format!("{}{}", target.base, stop.path);
        let mut req = match stop.method {
            "DELETE" => http.delete(url),
            _ => http.post(url).json(stop.body.as_ref().unwrap_or(&serde_json::json!({}))),
        };
        req = req.timeout(ASK);
        if !target.token.is_empty() {
            req = req.bearer_auth(&target.token);
        }
        match req.send().await {
            Ok(r) if r.status().is_success() => {}
            Ok(r) => failed.push(format!("{}: the mixer answered {}", thing.line, r.status())),
            Err(e) => failed.push(format!("{}: {e}", thing.line)),
        }
    }
    failed
}
