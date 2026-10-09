//! WHIP offers that arrived on the control port, handed to the listener.
//!
//! The HTTP is `crate::control::whip`; the key check and the WebRTC session
//! are the ingest plugin's, which answers `whip.offer` with the SDP answer
//! or with the status and sentence to refuse with. Both go as `tool.call`,
//! the one call a device takes besides the standard ones; neither is in the
//! plugin's `[[tools]]`, so no agent is offered them.

use serde_json::{json, Value};

use super::{net, Channels, PLUGIN};

/// The name of the provide that takes the offer.
const PROVIDE: &str = "discover";

/// What a WHIP request is answered with.
#[derive(Debug, Clone, PartialEq)]
pub enum Whip {
    Answer { session: String, sdp: String },
    Refused { status: u16, why: String },
}

impl Channels {
    /// Hand a publisher's offer to the listener. Blocks for up to the
    /// protocol's five seconds while WebRTC gathers its candidates.
    pub fn whip_offer(&self, app: &str, stream: &str, key: &str, sdp: &str, peer: &str) -> Whip {
        if !self.plugins().is_running(PLUGIN) {
            return Whip::Refused { status: 503, why: net::why_not_listening(PLUGIN) };
        }
        let params = json!({"app": app, "stream": stream, "key": key, "sdp": sdp, "peer": peer});
        match self.plugins().call_provide(PLUGIN, PROVIDE, "tool.call", json!({"name": "whip.offer", "arguments": params})) {
            Ok(answer) => read(&answer),
            Err(e) => Whip::Refused { status: 502, why: format!("{e:#}") },
        }
    }

    /// The publisher is done (`DELETE` on its session).
    pub fn whip_end(&self, session: &str) -> bool {
        let arguments = json!({"session": session});
        let answer = self.plugins().call_provide(PLUGIN, PROVIDE, "tool.call", json!({"name": "whip.end", "arguments": arguments}));
        answer.map(|a| a["ended"] == true).unwrap_or(false)
    }
}

fn read(answer: &Value) -> Whip {
    let text = |k: &str| answer[k].as_str().unwrap_or_default().to_string();
    match answer["status"].as_u64() {
        Some(status) => Whip::Refused { status: status as u16, why: text("why") },
        None if !text("sdp").is_empty() => Whip::Answer { session: text("session"), sdp: text("sdp") },
        None => Whip::Refused { status: 502, why: "the channel server gave no answer to the offer".into() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_and_a_refusal_are_told_apart() {
        assert_eq!(
            read(&json!({"session": "s1", "sdp": "v=0"})),
            Whip::Answer { session: "s1".into(), sdp: "v=0".into() }
        );
        assert_eq!(
            read(&json!({"status": 403, "why": "no"})),
            Whip::Refused { status: 403, why: "no".into() }
        );
        assert!(matches!(read(&json!({})), Whip::Refused { status: 502, .. }));
    }
}
