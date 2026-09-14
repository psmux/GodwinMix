//! The event stream in, packets out.
//!
//! `event/tally` is an `ext` stream: the core derives it, and it derives it
//! only because this plugin asked. Every change becomes one packet per lamp
//! whose colour moved, plus a whole set on a timer so a lamp that was power
//! cycled catches up without anyone taking a source.

use std::collections::BTreeMap;
use std::sync::Arc;

use godwinmix_client::{Client, Event};
use serde_json::json;
use tokio::sync::watch;

use crate::sender::Sender;
use crate::settings::Settings;
use crate::tsl::{self, Display, Lamp};

/// Where log lines go: the core's log in a sidecar, stderr by hand.
pub trait Log: Send + Sync + 'static {
    fn info(&self, message: &str);
    fn warn(&self, message: &str);
}

pub struct Stderr;

impl Log for Stderr {
    fn info(&self, message: &str) {
        eprintln!("gmx-tally: {message}");
    }
    fn warn(&self, message: &str) {
        eprintln!("gmx-tally: {message}");
    }
}

pub struct Wiring {
    pub url: String,
    pub token: Option<String>,
    pub log: Arc<dyn Log>,
}

/// What every lamp is showing, so a refresh sends the truth and a change sends
/// only what moved.
#[derive(Debug, Default, Clone)]
pub struct Board {
    states: BTreeMap<String, String>,
}

impl Board {
    /// Take in a whole tally document. Answers with the source ids whose state
    /// is different from what was already on the board.
    pub fn apply(&mut self, sources: &BTreeMap<String, serde_json::Value>) -> Vec<String> {
        let mut changed = Vec::new();
        for (id, state) in sources {
            let Some(state) = state.as_str() else { continue };
            if self.states.get(id).map(String::as_str) != Some(state) {
                self.states.insert(id.clone(), state.to_string());
                changed.push(id.clone());
            }
        }
        // A source that has gone away goes dark.
        let gone: Vec<String> = self
            .states
            .keys()
            .filter(|id| !sources.contains_key(*id))
            .cloned()
            .collect();
        for id in gone {
            self.states.insert(id.clone(), "off".into());
            changed.push(id);
        }
        changed
    }

    pub fn state_of(&self, id: &str) -> &str {
        self.states.get(id).map(String::as_str).unwrap_or("off")
    }

    pub fn ids(&self) -> Vec<String> {
        self.states.keys().cloned().collect()
    }
}

/// The packet one lamp should be showing right now.
pub fn packet_for(settings: &Settings, board: &Board, source: &str) -> Option<Vec<u8>> {
    let lamp = settings.lamp_for(source)?;
    let colour = settings.colour_for(board.state_of(source));
    Some(tsl::encode(
        settings.screen,
        &Display {
            index: lamp.index,
            right: colour,
            text: colour,
            left: Lamp::Off,
            brightness: settings.brightness,
            label: lamp.label.clone(),
        },
        settings.unicode,
    ))
}

/// Run until the process is told to stop.
pub async fn run(wiring: Wiring, mut settings: watch::Receiver<Settings>) {
    loop {
        let current = settings.borrow().clone();
        let client = match Client::connect(&wiring.url, wiring.token.as_deref()).await {
            Ok(client) => client,
            Err(error) => {
                wiring.log.warn(&format!(
                    "cannot reach the core at {}: {error}. Trying again in two seconds.",
                    wiring.url
                ));
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };
        // Nothing runs unless asked: this is the asking for event/tally.
        if let Err(error) = client.subscribe(&["tally", "source.state"], json!({"tally": true})).await {
            wiring.log.warn(&format!("the core refused core.subscribe: {error}"));
        }
        let reason = serve(&wiring, &client, &current, &mut settings).await;
        client.close();
        wiring.log.info(&format!("{reason}; reconnecting"));
    }
}

async fn serve(
    wiring: &Wiring,
    client: &Client,
    settings: &Settings,
    changes: &mut watch::Receiver<Settings>,
) -> String {
    let mut sender = match Sender::open(settings.protocol, &settings.address).await {
        Ok(sender) => sender,
        Err(error) => {
            wiring.log.warn(&format!(
                "cannot open a {} socket for {}: {error}",
                settings.protocol.name(),
                settings.address
            ));
            changes.changed().await.ok();
            return "the settings changed".into();
        }
    };
    wiring.log.info(&format!(
        "{} lamp(s) on {}",
        settings.lamps.len(),
        sender.describe()
    ));

    let mut board = Board::default();
    let mut events = client.events();
    let refresh = std::time::Duration::from_secs(settings.refresh_secs.max(1));
    let mut timer = tokio::time::interval(refresh);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(Event::Tally(tally)) => {
                    let changed = board.apply(&tally.sources);
                    send_these(wiring, &mut sender, settings, &board, &changed).await;
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    wiring.log.warn(&format!("fell {n} events behind; the refresh will put every lamp right"));
                }
                Err(_) => return "the core closed the event stream".into(),
            },
            _ = timer.tick(), if settings.refresh_secs > 0 => {
                let all = board.ids();
                send_these(wiring, &mut sender, settings, &board, &all).await;
            }
            _ = changes.changed() => return "the settings changed".into(),
        }
    }
}

async fn send_these(
    wiring: &Wiring,
    sender: &mut Sender,
    settings: &Settings,
    board: &Board,
    sources: &[String],
) {
    for source in sources {
        let Some(packet) = packet_for(settings, board, source) else {
            continue;
        };
        if let Err(error) = sender.send(&packet).await {
            wiring.log.warn(&format!(
                "could not send the lamp for '{source}' to {}: {error}",
                sender.describe()
            ));
            // One failure is enough: the rest of this batch would fail the same
            // way, and the refresh timer will try again.
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn tally(pairs: &[(&str, &str)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect()
    }

    fn settings() -> Settings {
        Settings::from_value(&json!({
            "lamps": [{"source": "cam1", "index": 0, "label": "CAM 1"},
                      {"source": "cam2", "index": 1, "label": "CAM 2"}]
        }))
    }

    #[test]
    fn the_first_document_changes_every_lamp() {
        let mut board = Board::default();
        let mut changed = board.apply(&tally(&[("cam1", "program"), ("cam2", "off")]));
        changed.sort();
        assert_eq!(changed, vec!["cam1", "cam2"]);
    }

    #[test]
    fn only_what_moved_is_sent_again() {
        let mut board = Board::default();
        board.apply(&tally(&[("cam1", "program"), ("cam2", "off")]));
        let changed = board.apply(&tally(&[("cam1", "off"), ("cam2", "program")]));
        assert_eq!(changed.len(), 2);
        let none = board.apply(&tally(&[("cam1", "off"), ("cam2", "program")]));
        assert!(none.is_empty(), "nothing moved, so nothing is sent");
    }

    #[test]
    fn a_source_that_goes_away_goes_dark() {
        let mut board = Board::default();
        board.apply(&tally(&[("cam1", "program"), ("cam2", "preview")]));
        let changed = board.apply(&tally(&[("cam1", "program")]));
        assert_eq!(changed, vec!["cam2"]);
        assert_eq!(board.state_of("cam2"), "off");
    }

    #[test]
    fn a_programme_source_lights_red_on_the_index_it_was_given() {
        let settings = settings();
        let mut board = Board::default();
        board.apply(&tally(&[("cam1", "program"), ("cam2", "preview")]));

        let packet = packet_for(&settings, &board, "cam1").expect("a packet");
        let read = tsl::decode(&packet).expect("a well formed packet");
        assert_eq!(read.displays[0].index, 0);
        assert_eq!(read.displays[0].right, Lamp::Red);
        assert_eq!(read.displays[0].label, "CAM 1");

        let packet = packet_for(&settings, &board, "cam2").expect("a packet");
        let read = tsl::decode(&packet).unwrap();
        assert_eq!(read.displays[0].index, 1);
        assert_eq!(read.displays[0].right, Lamp::Green);
    }

    #[test]
    fn a_source_with_no_lamp_produces_no_packet() {
        let settings = settings();
        let mut board = Board::default();
        board.apply(&tally(&[("cam9", "program")]));
        assert!(packet_for(&settings, &board, "cam9").is_none());
    }

    #[test]
    fn a_source_nobody_has_mentioned_reads_as_off() {
        let board = Board::default();
        assert_eq!(board.state_of("cam1"), "off");
        let packet = packet_for(&settings(), &board, "cam1").expect("a packet");
        let read = tsl::decode(&packet).unwrap();
        assert_eq!(read.displays[0].right, Lamp::Off);
    }

    #[test]
    fn a_non_string_tally_value_is_ignored_rather_than_guessed() {
        let mut board = Board::default();
        let mut document = tally(&[("cam1", "program")]);
        document.insert("cam2".into(), Value::Bool(true));
        let changed = board.apply(&document);
        assert_eq!(changed, vec!["cam1"]);
    }
}
