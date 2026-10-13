//! The station's shows as the channels' programme feeds reach them: each
//! show over its own socket, as `switch.rs` asks one to add an output. See
//! `channels/feed_loop.rs`.

use super::state::Station;
use crate::channels::feed_loop::{Answer, Asker};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::ShowState;
use serde_json::Value;
use std::sync::Weak;
use std::time::Duration;

/// How long one call to a show may take.
const ASK: Duration = Duration::from_secs(15);

pub struct Shows(pub Weak<Station>);

impl Asker for Shows {
    fn first(&self) -> String {
        self.0.upgrade().map(|st| st.first()).unwrap_or_default()
    }

    /// Every show with compositing on; one without has no programme.
    fn shows(&self) -> Vec<(String, Option<String>)> {
        let Some(st) = self.0.upgrade() else { return Vec::new() };
        let ids = st.registry.lock().ids();
        ids.into_iter()
            .filter(|id| !st.is_direct(id))
            .map(|id| {
                let why = match st.state_of(&id) {
                    Some(ShowState::Running) => None,
                    Some(ShowState::Stopped) => Some("it is stopped; start it on the Shows page".to_string()),
                    Some(ShowState::Failed) => Some("it stopped after failing; see the Shows page".to_string()),
                    _ => Some("it is starting".to_string()),
                };
                (id, why)
            })
            .collect()
    }

    fn ask<'a>(&'a self, show: &'a str, method: &'a str, params: Value) -> Answer<'a> {
        Box::pin(async move {
            let st = self.0.upgrade().ok_or_else(|| RpcError::not_in_state("the station is stopping"))?;
            st.ask_show(show, method, params, ASK).await
        })
    }
}
