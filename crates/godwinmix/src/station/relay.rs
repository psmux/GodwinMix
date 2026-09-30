//! Reaching one show: which one a request names, and where it is.
//!
//! Every path a client knows is the same under a station: `?show=<id>` picks
//! the show, and without it the first show answers. The station passes the
//! request to that show's own socket on loopback and passes the answer back,
//! byte for byte, streaming both ways: nothing is decoded, copied into a
//! buffer, or held. The three doors are `http.rs` (REST and the byte streams),
//! `pipe.rs` (a WebSocket other than `/rpc`) and `rpc.rs` (`/rpc`, which also
//! answers the station's own methods and carries the station's events).

pub mod http;
pub mod pipe;
pub mod rpc;

use super::state::Station;
use godwinmix_protocol::action::{ActionKind, ErrorAction};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::ShowState;
use std::net::SocketAddr;
use std::time::Duration;

/// How long a call waits for a show that is starting.
pub const START_WAIT: Duration = Duration::from_secs(15);

/// The show a query string names, if it names one.
pub fn show_in(query: Option<&str>) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == "show" && !v.is_empty()).then(|| v.to_string())
    })
}

/// The query string without `show`, for the show itself, which has no such
/// parameter and would read it as a method's.
pub fn without_show(query: Option<&str>) -> Option<String> {
    let kept: Vec<&str> = query?.split('&').filter(|p| !p.is_empty() && !p.starts_with("show=")).collect();
    (!kept.is_empty()).then(|| kept.join("&"))
}

impl Station {
    /// Where `id` answers, waiting up to [`START_WAIT`] while it starts.
    pub async fn addr_of(&self, id: &str) -> Result<SocketAddr, RpcError> {
        let (mut rx, state) = {
            let procs = self.procs.lock();
            let Some(p) = procs.get(id) else {
                let ids = self.registry.lock().ids();
                return Err(RpcError::not_found("show", id, &ids));
            };
            (p.addr.subscribe(), p.state)
        };
        if let Some(addr) = *rx.borrow() {
            return Ok(addr);
        }
        if matches!(state, ShowState::Stopped | ShowState::Failed) {
            return Err(not_running(id, state, self.procs.lock().get(id).and_then(|p| p.error.clone())));
        }
        let waited = tokio::time::timeout(START_WAIT, rx.wait_for(Option::is_some)).await;
        match waited {
            Ok(Ok(addr)) => addr.ok_or_else(|| starting(id)),
            _ => Err(starting(id)),
        }
    }
}

fn not_running(id: &str, state: ShowState, error: Option<String>) -> RpcError {
    // The shows panel is where a show is started; the value is what
    // show.start takes.
    let action = ErrorAction {
        panel: Some("shows".into()),
        value: Some(serde_json::json!({ "id": id })),
        ..ErrorAction::new("Start it", ActionKind::Open)
    };
    let why = error.map(|e| format!(" It failed: {e}")).unwrap_or_default();
    RpcError::not_in_state(format!("show {id} is not running.{why} Start it with show.start {{id: \"{id}\"}}."))
        .with("show", id)
        .with("state", serde_json::to_value(state).unwrap_or_default())
        .with_action(action)
}

fn starting(id: &str) -> RpcError {
    RpcError::not_in_state(format!(
        "show {id} is still starting after {} seconds. Try again in a moment; show.list says when it is running.",
        START_WAIT.as_secs()
    ))
    .with("show", id)
    .with("state", "starting")
    .with("retry_after_ms", 2000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_show_is_read_from_the_query_and_taken_out_before_the_show_sees_it() {
        assert_eq!(show_in(Some("show=b&token=t")), Some("b".into()));
        assert_eq!(show_in(Some("token=t")), None);
        assert_eq!(show_in(None), None);
        assert_eq!(without_show(Some("show=b&token=t")), Some("token=t".into()));
        assert_eq!(without_show(Some("show=b")), None);
    }
}
