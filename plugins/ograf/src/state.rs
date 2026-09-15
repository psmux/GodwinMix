//! What each graphic on the canvas is showing, kept here and not in the page.
//!
//! A browser source is a process the supervisor may restart: it stalls, it is
//! brought back, and the page loads again from nothing. If the words lived in
//! the page they would be gone, and the strap that was on air would come back
//! blank. So the state is here, the page asks for it on connect, and a restart
//! is invisible.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};
use tokio::sync::broadcast;

/// How many actions the channel holds for a page that is slow to read. A page
/// further behind than this reloads its whole state, which is `resync`.
const QUEUE: usize = 64;

/// What one graphic placement is doing.
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    /// The graphic it shows, `ograf/lower-third`.
    pub graphic: String,
    /// The values it was loaded or updated with.
    pub values: Map<String, Value>,
    /// Which step `playAction` has reached. 0 is off.
    pub step: u32,
    pub playing: bool,
    /// Bumped on every action, so a page can tell whether it has missed one.
    pub seq: u64,
}

impl Instance {
    fn new(graphic: &str) -> Instance {
        Instance {
            graphic: graphic.to_string(),
            values: Map::new(),
            step: 0,
            playing: false,
            seq: 0,
        }
    }

    /// What `/state/<instance>` answers and what a page applies on connect.
    pub fn as_json(&self, instance: &str) -> Value {
        json!({
            "instance": instance,
            "graphic": self.graphic,
            "values": self.values,
            "step": self.step,
            "playing": self.playing,
            "seq": self.seq,
        })
    }
}

/// One action, on its way to a page.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub instance: String,
    /// `load`, `update`, `play` or `stop`, which are the OGraf method names
    /// with `Action` taken off.
    pub verb: String,
    pub body: Value,
}

/// Every graphic this host is driving.
#[derive(Clone)]
pub struct Host {
    inner: Arc<Mutex<BTreeMap<String, Instance>>>,
    actions: broadcast::Sender<Action>,
}

impl Default for Host {
    fn default() -> Self {
        Host::new()
    }
}

impl Host {
    pub fn new() -> Host {
        Host {
            inner: Arc::new(Mutex::new(BTreeMap::new())),
            actions: broadcast::channel(QUEUE).0,
        }
    }

    /// The map, with a poisoned lock treated as an empty one.
    ///
    /// A panic inside a graphics host must not take the whole host down: the
    /// graphic that was on air stays on air because the browser holds the
    /// picture, and the next call works. `unwrap_or_else(|e| e.into_inner())`
    /// is the standard way to say that and it is what this needs.
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Instance>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Follow every action, for one page's event stream.
    pub fn follow(&self) -> broadcast::Receiver<Action> {
        self.actions.subscribe()
    }

    pub fn get(&self, instance: &str) -> Option<Instance> {
        self.lock().get(instance).cloned()
    }

    /// Every instance, for `status` and for the index page.
    pub fn all(&self) -> Vec<(String, Instance)> {
        self.lock()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// `load`: the graphic this placement shows, and the words to start with.
    ///
    /// Loading the same graphic again keeps the values it already had and
    /// merges the new ones over them, so a reload after a restart is not a
    /// blank strap. Loading a *different* graphic starts from nothing, because
    /// two templates do not share a schema.
    pub fn load(&self, instance: &str, graphic: &str, values: &Map<String, Value>) -> Instance {
        let mut inner = self.lock();
        let entry = inner
            .entry(instance.to_string())
            .or_insert_with(|| Instance::new(graphic));
        if entry.graphic != graphic {
            *entry = Instance::new(graphic);
        }
        for (key, value) in values {
            entry.values.insert(key.clone(), value.clone());
        }
        entry.seq += 1;
        let state = entry.clone();
        drop(inner);
        self.send(instance, "load", state.as_json(instance));
        state
    }

    /// `update`: new words, without replaying the animation.
    pub fn update(&self, instance: &str, values: &Map<String, Value>) -> Option<Instance> {
        let mut inner = self.lock();
        let entry = inner.get_mut(instance)?;
        for (key, value) in values {
            entry.values.insert(key.clone(), value.clone());
        }
        entry.seq += 1;
        let state = entry.clone();
        drop(inner);
        self.send(instance, "update", state.as_json(instance));
        Some(state)
    }

    /// `play`: the next step, or the step named.
    pub fn play(&self, instance: &str, step: Option<u32>) -> Option<Instance> {
        let mut inner = self.lock();
        let entry = inner.get_mut(instance)?;
        entry.step = step.unwrap_or(entry.step.saturating_add(1));
        entry.playing = true;
        entry.seq += 1;
        let state = entry.clone();
        drop(inner);
        self.send(instance, "play", state.as_json(instance));
        Some(state)
    }

    /// `stop`: off, and back to step 0.
    pub fn stop(&self, instance: &str) -> Option<Instance> {
        let mut inner = self.lock();
        let entry = inner.get_mut(instance)?;
        entry.step = 0;
        entry.playing = false;
        entry.seq += 1;
        let state = entry.clone();
        drop(inner);
        self.send(instance, "stop", state.as_json(instance));
        Some(state)
    }

    /// Forget an instance whose item was taken off the canvas.
    pub fn forget(&self, instance: &str) -> bool {
        self.lock().remove(instance).is_some()
    }

    /// A send with nobody listening is not a failure: a graphic can be loaded
    /// before its page is up, which is the ordinary order of things when a
    /// scene is built before it goes on air.
    fn send(&self, instance: &str, verb: &str, body: Value) {
        let _ = self.actions.send(Action {
            instance: instance.to_string(),
            verb: verb.to_string(),
            body,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn loading_the_same_graphic_again_keeps_the_words_it_had() {
        let host = Host::new();
        host.load("a", "ograf/lower-third", &values(&[("name", json!("Ada"))]));
        let back = host.load(
            "a",
            "ograf/lower-third",
            &values(&[("title", json!("Analyst"))]),
        );
        assert_eq!(
            back.values["name"], "Ada",
            "a restart must not blank the strap"
        );
        assert_eq!(back.values["title"], "Analyst");
    }

    #[test]
    fn loading_a_different_graphic_starts_from_nothing() {
        let host = Host::new();
        host.load("a", "ograf/lower-third", &values(&[("name", json!("Ada"))]));
        let back = host.load("a", "ograf/clock", &values(&[]));
        assert!(
            back.values.is_empty(),
            "two templates do not share a schema"
        );
        assert_eq!(back.graphic, "ograf/clock");
    }

    #[test]
    fn play_walks_the_steps_and_stop_puts_it_back_to_off() {
        let host = Host::new();
        host.load("a", "ograf/lower-third", &Map::new());
        assert_eq!(host.play("a", None).unwrap().step, 1);
        assert_eq!(host.play("a", None).unwrap().step, 2);
        assert_eq!(
            host.play("a", Some(1)).unwrap().step,
            1,
            "a named step is taken as given"
        );
        let off = host.stop("a").unwrap();
        assert_eq!(off.step, 0);
        assert!(!off.playing);
    }

    #[test]
    fn every_action_bumps_the_sequence_so_a_page_knows_it_missed_one() {
        let host = Host::new();
        let first = host.load("a", "ograf/lower-third", &Map::new()).seq;
        let last = host.stop("a").unwrap().seq;
        assert!(last > first);
    }

    #[test]
    fn driving_an_instance_that_was_never_loaded_answers_none_rather_than_inventing_one() {
        let host = Host::new();
        assert!(host.update("nobody", &Map::new()).is_none());
        assert!(host.play("nobody", None).is_none());
        assert!(host.stop("nobody").is_none());
    }

    #[test]
    fn a_page_that_is_listening_is_told_what_happened() {
        let host = Host::new();
        let mut watch = host.follow();
        host.load("a", "ograf/lower-third", &values(&[("name", json!("Ada"))]));
        let action = watch.try_recv().expect("the page was told");
        assert_eq!(action.verb, "load");
        assert_eq!(action.instance, "a");
        assert_eq!(action.body["values"]["name"], "Ada");
    }

    #[test]
    fn loading_with_nobody_listening_is_not_a_failure() {
        let host = Host::new();
        // A scene is usually built before it goes on air, so the page is not
        // up yet. The state is kept and the page picks it up on connect.
        let state = host.load("a", "ograf/lower-third", &values(&[("name", json!("Ada"))]));
        assert_eq!(state.values["name"], "Ada");
        assert_eq!(host.get("a").unwrap().values["name"], "Ada");
    }
}
