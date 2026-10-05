//! One connection, as the others see it.

use super::device_of;
use godwinmix_protocol::presence::PresenceClient;
use godwinmix_protocol::scope::Token;

#[derive(Debug, Clone)]
pub(super) struct Seat {
    pub client_id: String,
    token: String,
    /// The token's own label, when it has one.
    token_label: Option<String>,
    /// The name this connection gave itself with presence.set.
    pub label: Option<String>,
    device: String,
    pub scene: Option<String>,
    since_ms: u64,
}

impl Seat {
    pub fn new(client_id: &str, token: &Token, user_agent: Option<&str>) -> Seat {
        Seat {
            client_id: client_id.to_string(),
            token: token.id.clone(),
            token_label: token_label(token),
            label: None,
            device: device_of(user_agent.unwrap_or("")),
            scene: None,
            since_ms: now_ms(),
        }
    }

    /// As `presence.list` shows it, with `you` set when `asking` is this one.
    pub fn listed(&self, asking: Option<&str>) -> PresenceClient {
        PresenceClient {
            client_id: self.client_id.clone(),
            token: self.token.clone(),
            label: self.label(),
            device: self.device.clone(),
            scene: self.scene.clone(),
            since_ms: self.since_ms,
            you: asking == Some(self.client_id.as_str()),
        }
    }

    /// Its label, else its device, else nothing: how to name it to a person.
    pub fn name(&self) -> Option<String> {
        match (self.label(), self.device.as_str()) {
            (Some(l), "") => Some(l),
            (Some(l), d) => Some(format!("{l} ({d})")),
            (None, "") => None,
            (None, d) => Some(d.to_string()),
        }
    }

    fn label(&self) -> Option<String> {
        self.label.clone().or_else(|| self.token_label.clone())
    }
}

/// The label a token carries, for presence to show beside its connections.
/// Tokens from the config file have none; a token minted at run time with a
/// label is where one comes from.
fn token_label(_token: &Token) -> Option<String> {
    None
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
