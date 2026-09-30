//! `udp/source`: the `Source` methods, and the errors they answer with.

use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use crate::recv::settings::Settings;
use crate::recv::{Receiver, Sink};

pub struct UdpSource {
    settings: Settings,
    reporter: Option<Reporter>,
    receiver: Option<Receiver>,
}

impl UdpSource {
    pub fn new() -> UdpSource {
        UdpSource { settings: Settings::default(), reporter: None, receiver: None }
    }
}

fn invalid(message: String) -> RpcError {
    RpcError::new(codes::INVALID_PARAMS, message)
}

impl Source for UdpSource {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        self.settings = Settings::from_params(&ready.params).map_err(invalid)?;
        gmx_netkit::init().map_err(internal)?;
        gmx_netkit::elements::require(&["udpsrc"]).map_err(internal)?;
        let address = self.settings.endpoint().map_err(invalid)?.display(self.settings.scheme());
        reporter.info(format!("udp/source '{}' will receive {address}", ready.instance));
        self.reporter = Some(reporter);
        // No jitter buffer and no retransmission: what arrives is passed on
        // as it arrives, so this source adds nothing the aligner must absorb.
        Ok(InitializeResult { latency_ms: Some(0) })
    }

    /// The socket opens here and not before, so a source that is added and
    /// never started holds no port.
    fn start(&mut self, _params: &StartParams) -> Result<StartResult, RpcError> {
        self.receiver = None;
        let receiver = Receiver::start(&self.settings, self.reporter.clone(), Sink::Stdout).map_err(internal)?;
        self.receiver = Some(receiver);
        Ok(StartResult { latency_ms: Some(0) })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.receiver = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = Settings::from_params(&params).map_err(invalid)?;
        if wanted == self.settings {
            return Ok(Configure::applied());
        }
        self.settings = wanted;
        if self.receiver.is_none() {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required(
            "a UDP receiver binds its port and joins its group once. Call plugin.reload and \
             it opens again with the new address, interface or program.",
        ))
    }

    fn health(&mut self) -> Health {
        match &self.receiver {
            Some(r) => r.health(),
            None => Health::degraded("not started yet, so no port is open"),
        }
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        let r = self.receiver.as_ref();
        match method {
            "stats" => Ok(json!({
                "address": r.map(|r| r.address().to_string()),
                "stats": r.map(Receiver::stats).unwrap_or(Value::Null),
            })),
            "programs" => Ok(r.map(Receiver::programs).unwrap_or_else(|| json!({"programs": [], "chosen": null}))),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!(
                    "udp/source has no method '{other}'. It answers 'stats' and 'programs', and \
                     the standard source methods in docs/reference/plugin-protocol.md."
                ),
            )),
        }
    }
}

pub fn internal(message: String) -> RpcError {
    RpcError::new(codes::INTERNAL_ERROR, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_sdk::wire::{Canvas, HealthState, Transport};

    fn ready(params: Value) -> Ready {
        let mut ready = Ready::for_test(Canvas::new(1280, 720, 30));
        ready.instance = "feed".into();
        ready.provide = "source".into();
        ready.transport = Transport::Container;
        ready.params = params;
        ready
    }

    #[test]
    fn nonsense_settings_are_refused_at_initialize_with_minus_32602() {
        let mut s = UdpSource::new();
        let err = s.initialize(&ready(json!({"uri": "http://nope"})), Reporter::for_test()).unwrap_err();
        assert_eq!(err.code, codes::INVALID_PARAMS);
        assert!(err.message.contains("udp://@239.1.1.1:5000"), "{}", err.message);
    }

    #[test]
    fn initialize_opens_no_port_and_health_says_so() {
        let mut s = UdpSource::new();
        s.initialize(&ready(json!({"uri": "udp://@239.1.1.1:5000"})), Reporter::for_test()).unwrap();
        let h = s.health();
        assert_eq!(h.state, HealthState::Degraded);
        assert!(h.detail.unwrap().contains("no port is open"));
    }

    #[test]
    fn a_new_program_while_stopped_is_applied() {
        let mut s = UdpSource::new();
        assert!(s.configure(json!({"program": 3})).unwrap().applied);
        assert_eq!(s.settings.program, 3);
    }

    #[test]
    fn an_unknown_method_names_the_two_this_answers() {
        let err = UdpSource::new().call("teleport", Value::Null).unwrap_err();
        assert!(err.message.contains("'programs'"), "{}", err.message);
    }
}
