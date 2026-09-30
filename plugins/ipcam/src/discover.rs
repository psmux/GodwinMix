//! `ipcam/discover`: ONVIF cameras on the LAN, as ready to add streams.
//!
//! Each camera profile becomes a candidate of the core's own `hls/source`
//! with its RTSP address, so the picture is pulled and decoded by the core as
//! any RTSP stream is, and this plugin moves no media at all. A camera that
//! will not say its addresses without a login is named in health, with where
//! to put the login.

use std::net::SocketAddr;
use std::time::Duration;

use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use crate::onvif::{self, soap::Login, wsd};

#[derive(Default)]
pub struct Discover {
    login: Login,
    locked: Vec<String>,
    /// Where the probe goes. The ONVIF group unless a test says otherwise.
    to: Option<SocketAddr>,
}

fn login_of(params: &Value) -> Login {
    let text = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Login { user: text("user"), password: text("password") }
}

impl Discover {
    /// The candidates for one discovery.
    pub fn candidates(&mut self, wait: Duration) -> Vec<Candidate> {
        let to = self.to.unwrap_or_else(|| wsd::MULTICAST.parse().expect("a valid group address"));
        let found = onvif::discover(to, wait, &self.login);
        self.locked = found.locked;
        found
            .streams
            .into_iter()
            .map(|s| Candidate { kind: "hls/source".into(), name: s.name, params: json!({ "uri": s.uri }), confidence: 0.9 })
            .collect()
    }

    #[cfg(test)]
    pub fn aimed(to: SocketAddr, login: Login) -> Discover {
        Discover { login, locked: Vec::new(), to: Some(to) }
    }
}

impl Device for Discover {
    fn initialize(&mut self, ready: &Ready, _reporter: Reporter) -> Result<InitializeResult, RpcError> {
        self.login = login_of(&ready.params);
        Ok(InitializeResult::default())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        self.login = login_of(&params);
        Ok(Configure::applied())
    }

    fn health(&mut self) -> Health {
        if self.locked.is_empty() {
            return Health::ok();
        }
        Health::degraded(format!(
            "these cameras would not give their stream addresses without a login ({} of them): {}. Put the cameras' user name and password in the IP camera plugin's settings and look again.",
            self.locked.len(),
            self.locked.join(", ")
        ))
    }

    fn discover(&mut self, timeout_ms: u64) -> Result<Vec<Candidate>, RpcError> {
        // Inside the core's wait, with a tenth to spare for the answer.
        Ok(self.candidates(Duration::from_millis(timeout_ms.max(200) * 9 / 10)))
    }
}
