//! Who is connected to `/rpc`, for `presence.list` and `event/presence.changed`.
//!
//! Each connection takes a seat when it opens and gives it back when it
//! closes, which is a map insert and a map remove. Nothing else happens unless
//! somebody asked: the list is built when `presence.list` is called or when a
//! subscriber is told it changed, and a change is announced only while at
//! least one connection is subscribed to `presence.changed`.

use godwinmix_protocol::presence::{PresenceClient, PresenceList};
use godwinmix_protocol::scope::Token;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

/// One connection, as the others see it.
#[derive(Debug, Clone)]
struct Seat {
    client_id: String,
    token: String,
    /// The token's own label, when it has one.
    token_label: Option<String>,
    /// The name this connection gave itself with presence.set.
    label: Option<String>,
    device: String,
    scene: Option<String>,
    since_ms: u64,
}

#[derive(Debug)]
pub struct Presence {
    seats: Mutex<BTreeMap<u64, Seat>>,
    next: AtomicU64,
    changed: broadcast::Sender<()>,
}

impl Presence {
    pub fn new() -> Arc<Presence> {
        Arc::new(Presence {
            seats: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            changed: broadcast::channel(16).0,
        })
    }

    /// A name for a connection that did not choose one: `s1`, `s2`, and so
    /// on for the life of the process, so two connections never share one.
    pub fn fresh_name(&self) -> String {
        format!("s{}", self.next.fetch_add(1, Ordering::Relaxed))
    }

    /// Take a seat. Dropping what this returns gives it back.
    pub fn join(self: &Arc<Self>, client_id: &str, token: &Token, user_agent: Option<&str>) -> Here {
        let seat = Seat {
            client_id: client_id.to_string(),
            token: token.id.clone(),
            token_label: token_label(token),
            label: None,
            device: device_of(user_agent.unwrap_or("")),
            scene: None,
            since_ms: now_ms(),
        };
        let key = self.next.fetch_add(1, Ordering::Relaxed);
        self.seats.lock().insert(key, seat);
        self.announce();
        Here { presence: self.clone(), key }
    }

    /// Everybody connected, oldest first, with `you` set on `asking`.
    pub fn list(&self, asking: Option<&str>) -> PresenceList {
        let seats = self.seats.lock();
        let clients = seats
            .values()
            .map(|s| PresenceClient {
                client_id: s.client_id.clone(),
                token: s.token.clone(),
                label: s.label.clone().or_else(|| s.token_label.clone()),
                device: s.device.clone(),
                scene: s.scene.clone(),
                since_ms: s.since_ms,
                you: asking == Some(s.client_id.as_str()),
            })
            .collect();
        PresenceList { clients }
    }

    /// What a connection says about itself. False when no connection has
    /// this client id.
    pub fn set(&self, client_id: &str, scene: Option<String>, label: Option<String>) -> bool {
        let mut found = false;
        for seat in self.seats.lock().values_mut().filter(|s| s.client_id == client_id) {
            seat.scene = scene.clone();
            if let Some(label) = &label {
                seat.label = Some(label.trim().to_string()).filter(|l| !l.is_empty());
            }
            found = true;
        }
        if found {
            self.announce();
        }
        found
    }

    /// How to name a client to a person: its label, else its device, else
    /// nothing, when it is connected.
    pub fn who(&self, client_id: &str) -> Option<String> {
        let seats = self.seats.lock();
        let seat = seats.values().find(|s| s.client_id == client_id)?;
        let label = seat.label.clone().or_else(|| seat.token_label.clone());
        match (label, seat.device.as_str()) {
            (Some(l), "") => Some(l),
            (Some(l), d) => Some(format!("{l} ({d})")),
            (None, "") => None,
            (None, d) => Some(d.to_string()),
        }
    }

    /// Be told whenever the list changes. Holding a receiver is what makes a
    /// change announced at all.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }

    fn announce(&self) {
        if self.changed.receiver_count() > 0 {
            let _ = self.changed.send(());
        }
    }
}

/// A connection's seat, given back when the connection ends.
pub struct Here {
    presence: Arc<Presence>,
    key: u64,
}

impl Drop for Here {
    fn drop(&mut self) {
        self.presence.seats.lock().remove(&self.key);
        self.presence.announce();
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

/// A short guess at the device from a User-Agent: "iPhone Safari", "Android
/// Chrome", "Windows Edge", "gmx CLI". Never exact, and only ever shown to a
/// person, so a guess that reads well beats a string nobody can read.
pub fn device_of(agent: &str) -> String {
    let platforms = [
        ("iPhone", "iPhone"),
        ("iPad", "iPad"),
        ("Android", "Android"),
        ("Windows", "Windows"),
        ("Macintosh", "Mac"),
        ("CrOS", "Chromebook"),
        ("Linux", "Linux"),
    ];
    // Order matters: Edge and Opera say Chrome, and Chrome says Safari.
    let browsers = [
        ("Edg", "Edge"),
        ("OPR", "Opera"),
        ("Firefox", "Firefox"),
        ("FxiOS", "Firefox"),
        ("CriOS", "Chrome"),
        ("Chrome", "Chrome"),
        ("Safari", "Safari"),
    ];
    let platform = platforms.iter().find(|(k, _)| agent.contains(k)).map(|(_, v)| *v);
    let browser = browsers.iter().find(|(k, _)| agent.contains(k)).map(|(_, v)| *v);
    match (platform, browser) {
        (Some(p), Some(b)) => format!("{p} {b}"),
        (Some(p), None) => p.to_string(),
        (None, Some(b)) => b.to_string(),
        (None, None) if agent.starts_with("gmx") => "gmx CLI".into(),
        (None, None) => agent.split('/').next().unwrap_or("").trim().chars().take(32).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_agent_reads_as_a_device_a_person_recognises() {
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 \
                      (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
        assert_eq!(device_of(iphone), "iPhone Safari");
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like \
                    Gecko) Chrome/120.0 Safari/537.36 Edg/120.0";
        assert_eq!(device_of(edge), "Windows Edge");
        assert_eq!(device_of("tungstenite-rs"), "tungstenite-rs");
        assert_eq!(device_of(""), "");
    }

    #[test]
    fn a_seat_is_given_back_when_the_connection_goes() {
        let presence = Presence::new();
        let mut told = presence.subscribe();
        let token = Token::open();
        let here = presence.join("open.s1", &token, Some("gmx/0.2"));
        assert!(told.try_recv().is_ok(), "joining was not announced");
        assert!(presence.set("open.s1", Some("wide".into()), Some("Sam".into())));
        let list = presence.list(Some("open.s1"));
        assert_eq!(list.clients.len(), 1);
        assert!(list.clients[0].you);
        assert_eq!(list.clients[0].scene.as_deref(), Some("wide"));
        assert_eq!(presence.who("open.s1").as_deref(), Some("Sam (gmx CLI)"));
        drop(here);
        assert!(presence.list(None).clients.is_empty(), "the seat outlived its connection");
        assert!(!presence.set("open.s1", None, None), "nobody is there to describe");
    }
}
