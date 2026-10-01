//! The link between a show and its station.
//!
//! One TCP connection on loopback per show, JSON-RPC 2.0 one object per
//! line, opened by the show as soon as its control socket is bound. It
//! carries what a show asks of the station, never media and never a client's
//! call:
//!
//! ```text
//!   show.hello        {show, addr, secret, pid}   first line, no answer
//!   governor.admit    Ask            -> Answer    the one budget
//!   governor.release  {ticket}                    a node stopped
//!   show.on_air       {on}                        something started or stopped going out
//!   show.load         {millicores}                its own CPU, each second it holds a ticket
//!   show.health       {health}                    its programme's health, when its state or alarm kinds move
//!   governor.shed     {steps}                     station to show: what to give up now
//! ```
//!
//! When the connection closes the station drops every ticket it held for
//! that show, which is how a show that died gives its share back. When it
//! closes from the other side the show stops: a show nobody can reach is of
//! no use to anybody. See `client.rs` for the show's half.

pub mod client;
mod report;
pub mod serve;

use serde::{Deserialize, Serialize};

pub use serve::listen;

/// The first line a show sends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub show: String,
    /// Where its control socket is bound.
    pub addr: std::net::SocketAddr,
    /// What the station put in `GODWINMIX_STATION_SECRET` when it started
    /// this process, so nothing else on the machine can claim to be a show.
    pub secret: String,
    pub pid: u32,
}

/// The environment variable a show finds its secret in.
pub const SECRET_ENV: &str = "GODWINMIX_STATION_SECRET";

#[derive(Debug, Serialize, Deserialize)]
pub struct Line {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub result: serde_json::Value,
}

impl Line {
    pub fn call(id: Option<u64>, method: &str, params: serde_json::Value) -> Line {
        Line { id, method: Some(method.into()), params, result: serde_json::Value::Null }
    }

    pub fn reply(id: u64, result: serde_json::Value) -> Line {
        Line { id: Some(id), method: None, params: serde_json::Value::Null, result }
    }

    pub fn text(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_default();
        s.push('\n');
        s
    }
}

#[cfg(test)]
mod tests;
