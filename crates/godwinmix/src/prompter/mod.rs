//! The teleprompter: scripts kept beside the show's config, the one on the
//! prompter now, and where it is.
//!
//! What it costs: nothing at all between calls. There is no timer and no
//! task. A call settles the position to now, changes what it was asked to,
//! writes the file and puts one `prompter.changed` on the event bus, which
//! reaches only the clients that subscribed to it. Every screen scrolls by
//! itself from the position, the moment it was true and the speed, so a
//! prompter that plays for an hour sends nothing in that hour.

mod check;
mod scripts;
mod store;
mod transport;

use godwinmix_core::mixer::MixerHandle;
use godwinmix_protocol::prompter::{markers, rows, Look, Marker, PrompterState, Row, Script, DEFAULT_SPEED};
use godwinmix_protocol::types::Event;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct Prompter {
    inner: Mutex<Inner>,
    /// `<stem>.prompter.json` beside the config. `None` for a core with no
    /// config file, which keeps its scripts for as long as it runs.
    file: Option<PathBuf>,
    bus: MixerHandle,
}

struct Inner {
    scripts: BTreeMap<String, Script>,
    on: Option<String>,
    /// The script on the prompter, broken at `look.line_chars`.
    rows: Vec<Row>,
    markers: Vec<Marker>,
    position: f64,
    at_ms: u64,
    speed: f64,
    playing: bool,
    look: Look,
    revision: u64,
}

/// The mixer's clock, in milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Inner {
    fn empty() -> Inner {
        Inner {
            scripts: BTreeMap::new(),
            on: None,
            rows: Vec::new(),
            markers: Vec::new(),
            position: 0.0,
            at_ms: now_ms(),
            speed: DEFAULT_SPEED,
            playing: false,
            look: Look::default(),
            revision: 0,
        }
    }

    fn state(&self) -> PrompterState {
        let script = self.on.as_ref().and_then(|id| self.scripts.get(id));
        PrompterState {
            script: self.on.clone(),
            title: script.map(|s| s.title.clone()),
            updated_ms: script.map_or(0, |s| s.updated_ms),
            rows: self.rows.len() as u32,
            markers: self.markers.clone(),
            position: self.position,
            at_ms: self.at_ms,
            speed: self.speed,
            playing: self.playing,
            look: self.look,
            now_ms: now_ms(),
            revision: self.revision,
        }
    }

    /// Bring the position up to now, so a change starts from where every
    /// screen is rather than from where the last change left it.
    fn settle(&mut self) {
        let now = now_ms();
        self.position = self.state().position_at(now);
        self.at_ms = now;
    }

    /// Break the script on the prompter into rows again, after its text or
    /// the width changed.
    fn rebuild(&mut self) {
        let text = self.on.as_ref().and_then(|id| self.scripts.get(id)).map(|s| s.text.as_str()).unwrap_or("");
        self.rows = rows(text, self.look.line_chars);
        if self.on.is_none() {
            self.rows.clear();
        }
        self.markers = markers(&self.rows);
        self.position = self.position.clamp(0.0, self.rows.len() as f64);
    }
}

impl Prompter {
    /// Read what was saved beside `config`. Starts nothing.
    pub fn open(config: &Path, bus: MixerHandle) -> Arc<Prompter> {
        let file = (!config.as_os_str().is_empty()).then(|| store::path_beside(config));
        let inner = file.as_deref().map(store::load).unwrap_or_else(Inner::empty);
        Arc::new(Prompter { inner: Mutex::new(inner), file, bus })
    }

    pub fn state(&self) -> PrompterState {
        self.inner.lock().state()
    }

    /// Change the state, write it down when `keep` says so, and tell every
    /// subscriber. A speed change or a play is not written: something that
    /// follows the voice may send several a second, and a restart comes back
    /// paused at the last place the prompter stopped or was sent to.
    fn change<T>(&self, keep: bool, edit: impl FnOnce(&mut Inner) -> T) -> (T, PrompterState) {
        let (out, state, saved) = {
            let mut inner = self.inner.lock();
            inner.settle();
            let out = edit(&mut inner);
            inner.revision += 1;
            (out, inner.state(), keep.then(|| store::File::of(&inner)))
        };
        if let Some(saved) = saved {
            self.write(saved);
        }
        self.bus.emit(Event::PrompterChanged { state: Box::new(state.clone()) });
        (out, state)
    }

    fn write(&self, saved: store::File) {
        if let Some(path) = &self.file {
            store::write(path, &saved);
        }
    }
}
