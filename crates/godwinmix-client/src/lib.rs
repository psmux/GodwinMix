//! A client for the GodwinMix control protocol.
//!
//! The same contract the first party UI uses: one WebSocket at `/rpc`, JSON-RPC
//! over it, `core.subscribe` to say what you want, a snapshot then deltas, and
//! `event/flush` to say when to repaint.
//!
//! ```no_run
//! use godwinmix_client::Client;
//!
//! # async fn go() -> godwinmix_client::Result<()> {
//! let client = Client::connect("http://127.0.0.1:8080", Some("token")).await?;
//! client.subscribe(&["program.*", "source.*", "flush"], serde_json::json!({})).await?;
//! client.next_flush().await?;
//! for source in &client.state().status.sources {
//!     println!("{} {} {}", source.id, source.name, source.state);
//! }
//! client.take(Some("cam1")).await?;
//! # Ok(()) }
//! ```
//!
//! The typed methods, the types and the event enum in [`generated`] come from
//! `protocol.json` by way of `clients/gen/generate.py`. Everything else here is
//! hand written: the connection, the store, the frame decoder and the URLs.
//!
//! The major version of this crate is the `api_level` it speaks. Check a core
//! with `core.info`: it is safe when `api_compatible <= API_LEVEL <= api_level`.

mod client;
mod error;
pub mod frames;
mod generated;
pub mod store;
pub mod urls;

pub use client::Client;
pub use error::{codes, Error, Result};
pub use frames::{parse_frame, sheet_width_for, Frame, HEADER_BYTES};
pub use generated::*;
pub use store::State;

/// Every event a UI normally wants, for the common case of `subscribe`.
///
/// Patterns match the part after `event/`, and `*` matches one or more
/// characters, so `"source.*"` covers `event/source.state`. Expensive streams
/// are not in here: ask for those through `ext`.
pub const UI_EVENTS: [&str; 9] = [
    "snapshot",
    "program.*",
    "source.*",
    "output.*",
    "adbreak.*",
    "media.*",
    "alert",
    "resync",
    "flush",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_tables_are_there() {
        assert_eq!(API_LEVEL, 1);
        assert!(METHODS.iter().any(|m| m.name == "program.take"));
        assert!(EVENT_NAMES.contains(&"flush"));
        assert!(EXT_KEYS.contains(&"multiview"));
    }

    #[test]
    fn an_error_reads_its_own_next_step() {
        let error = Error::Rpc {
            code: codes::WRONG_STATE,
            message: "cam1 is connecting. Wait for it to go live, then take it.".into(),
            data: serde_json::json!({}),
        };
        assert!(error.retryable());
        assert_eq!(error.next_step(), Some("Wait for it to go live, then take it."));
    }
}
