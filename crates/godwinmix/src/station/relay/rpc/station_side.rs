//! The station's half of a relayed `/rpc`: its own methods, answered on a
//! task of their own, and its own events, written between the show's.

use super::super::super::methods;
use super::Relay;
use godwinmix_protocol::rpc;
use serde_json::{json, Value};
use tokio::sync::mpsc;

impl Relay {
    /// Run a station method on a task of its own, so a slow one (a show
    /// stopping takes seconds) holds up nothing else on this connection.
    pub(super) fn answer_later(&self, id: Option<Value>, method: String, params: Value, answers: mpsc::Sender<Value>) {
        let (st, token) = (self.st.clone(), self.token.clone());
        tokio::spawn(async move {
            let answer = methods::call(&st, &token, &method, params).await;
            let Some(id) = id else { return };
            let frame = match answer {
                Ok(v) => rpc::result_frame(&id, v),
                Err(e) => rpc::error_frame(&id, &e, ""),
            };
            let _ = answers.send(frame).await;
        });
    }

    pub(super) fn note_seq(&mut self, text: &str) {
        if text.contains("\"event/flush\"") {
            if let Some(seq) = serde_json::from_str::<Value>(text).ok().and_then(|v| v["params"]["seq"].as_u64()) {
                self.seq = seq;
            }
        }
    }

    /// A station event, and the flush after it, for a client that asked.
    pub(super) fn station_event(&self, event: &godwinmix_protocol::types::Event) -> Vec<Value> {
        let Some(sub) = self.sub.as_ref() else { return Vec::new() };
        let Some((name, payload)) = rpc::event_name_and_payload(event) else { return Vec::new() };
        if !sub.wants(name) {
            return Vec::new();
        }
        vec![rpc::notification(&format!("event/{name}"), payload), rpc::notification("event/flush", json!({ "seq": self.seq }))]
    }
}
