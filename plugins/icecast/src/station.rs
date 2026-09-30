//! `icecast/source`: the `Source` methods for a station played live.

use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use crate::handlers::{internal, invalid, no_method};
use crate::radio::{Radio, Sink};

#[derive(Default)]
pub struct RadioSource {
    uri: String,
    reporter: Option<Reporter>,
    radio: Option<Radio>,
}

fn radio_uri(params: &Value) -> Result<String, RpcError> {
    let uri = params.get("uri").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if !uri.is_empty() && !(uri.starts_with("http://") || uri.starts_with("https://")) {
        return Err(invalid(format!("icecast/source uri must be the station's http:// or https:// stream address, such as http://radio.example.com:8000/live.mp3; got {uri}")));
    }
    Ok(uri)
}

impl Source for RadioSource {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        self.uri = radio_uri(&ready.params)?;
        gmx_netkit::init().map_err(internal)?;
        self.reporter = Some(reporter);
        Ok(InitializeResult { latency_ms: Some(500) })
    }

    fn start(&mut self, _params: &StartParams) -> Result<StartResult, RpcError> {
        if self.uri.is_empty() {
            return Err(invalid("icecast/source needs the station's stream address in uri"));
        }
        self.radio = Some(Radio::start(&self.uri, Sink::Stdout, self.reporter.clone()).map_err(internal)?);
        Ok(StartResult { latency_ms: Some(500) })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.radio = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let uri = radio_uri(&params)?;
        let changed = uri != self.uri;
        self.uri = uri;
        if self.radio.is_none() || !changed {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required("a station takes a new address by being opened again"))
    }

    fn health(&mut self) -> Health {
        let Some(r) = &self.radio else { return Health::degraded("not started yet") };
        if let Some(f) = r.pipe.failure() {
            return Health::failing(format!("the station at {} stopped: {f}", self.uri));
        }
        let title = r.title.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut h = Health::ok();
        h.detail = Some(if title.is_empty() { format!("playing {}", self.uri) } else { format!("now playing: {title}") });
        h
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            "stats" => Ok(json!({"address": self.uri, "title": self.radio.as_ref().map(|r| r.title.lock().unwrap_or_else(|e| e.into_inner()).clone())})),
            other => Err(no_method("icecast/source", other)),
        }
    }
}
