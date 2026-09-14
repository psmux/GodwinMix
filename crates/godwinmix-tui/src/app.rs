//! What the operator's keys turn into, and what the core's answers turn into.
//!
//! Nothing here touches a terminal, which is why the tests can drive the whole
//! surface against a fake core with no screen at all.

use crate::client::{Command, Config, Frame, Incoming};
use crate::keys::Action;
use crate::model::{step_gain, OutputStatus, SourceStatus};
use crate::store::{Outcome, Store, View};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Sources,
    Outputs,
    Alerts,
}

impl Pane {
    fn next(self) -> Self {
        match self {
            Self::Sources => Self::Outputs,
            Self::Outputs => Self::Alerts,
            Self::Alerts => Self::Sources,
        }
    }

    fn prev(self) -> Self {
        self.next().next()
    }
}

/// What the screen is doing with the keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Typing a filter for the focused pane.
    Filter,
    /// Typing the clip an ad break should roll.
    AdUri,
    Help,
}

/// Where the link is. The screen says which, because stale numbers that look
/// live are worse than no numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Connecting,
    Live,
    Retrying { reason: String, in_secs: u64 },
}

/// The footer: one line, the last thing that happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    pub text: String,
    pub bad: bool,
}

impl Default for Footer {
    fn default() -> Self {
        Self { text: "? for keys, q to quit".into(), bad: false }
    }
}

pub struct App {
    pub store: Store,
    pub pane: Pane,
    pub mode: Mode,
    pub link: Link,
    pub footer: Footer,
    pub quit: bool,
    /// Set when something worth repainting happened. The mixer's own state
    /// only moves at `event/flush`, so this is what keeps a burst of deltas
    /// from painting five times.
    pub dirty: bool,
    /// The filter in force, and the line being typed into it.
    pub filter: String,
    pub input: String,
    pub sel_source: usize,
    pub sel_output: usize,
    pub sel_alert: usize,
    /// `ext` keys this build of the core ignored, from the subscribe answer.
    pub ignored_ext: Vec<String>,
    /// The last mosaic frame, when the operator asked for a picture.
    pub frame: Option<Frame>,
    pub frames_seen: u64,
    pub wants_picture: bool,
    /// Set once the core has refused `program.revert` as an unknown method,
    /// so the key says so instead of trying again.
    pub revert_missing: bool,
}

impl App {
    pub fn new(config: &Config) -> Self {
        Self {
            store: Store::new(),
            pane: Pane::Sources,
            mode: Mode::Normal,
            link: Link::Connecting,
            footer: Footer::default(),
            quit: false,
            dirty: true,
            filter: String::new(),
            input: String::new(),
            sel_source: 0,
            sel_output: 0,
            sel_alert: 0,
            ignored_ext: Vec::new(),
            frame: None,
            frames_seen: 0,
            wants_picture: config.multiview.is_some(),
            revert_missing: false,
        }
    }

    pub fn view(&self) -> &View {
        self.store.view()
    }

    /// The sources on screen, in snapshot order, with the filter applied. The
    /// number keys count these, so what `3` takes is the third row an operator
    /// can see and not the third row of a list they cannot.
    pub fn visible_sources(&self) -> Vec<&SourceStatus> {
        let needle = self.filter.to_lowercase();
        self.view()
            .status
            .sources
            .iter()
            .filter(|s| {
                needle.is_empty()
                    || s.id.to_lowercase().contains(&needle)
                    || s.name.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn visible_outputs(&self) -> Vec<&OutputStatus> {
        let needle = self.filter.to_lowercase();
        self.view()
            .status
            .outputs
            .iter()
            .filter(|o| {
                needle.is_empty()
                    || o.id.to_lowercase().contains(&needle)
                    || o.uri_host.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn selected_source(&self) -> Option<&SourceStatus> {
        self.visible_sources().get(self.sel_source).copied()
    }

    pub fn selected_output(&self) -> Option<&OutputStatus> {
        self.visible_outputs().get(self.sel_output).copied()
    }

    fn say(&mut self, text: impl Into<String>) {
        self.footer = Footer { text: text.into(), bad: false };
    }

    fn complain(&mut self, text: impl Into<String>) {
        self.footer = Footer { text: text.into(), bad: true };
    }

    /// One action from the keyboard, and the call it makes, if it makes one.
    pub fn act(&mut self, action: Action) -> Option<Command> {
        match self.mode {
            Mode::Filter | Mode::AdUri => self.act_typing(action),
            _ => self.act_normal(action),
        }
    }

    fn act_typing(&mut self, action: Action) -> Option<Command> {
        match action {
            Action::Type(c) => {
                self.input.push(c);
                if self.mode == Mode::Filter {
                    self.filter = self.input.clone();
                    self.clamp();
                }
            }
            Action::Backspace => {
                self.input.pop();
                if self.mode == Mode::Filter {
                    self.filter = self.input.clone();
                    self.clamp();
                }
            }
            Action::Cancel => {
                if self.mode == Mode::Filter {
                    self.filter.clear();
                }
                self.input.clear();
                self.mode = Mode::Normal;
            }
            Action::Accept => {
                let typed = std::mem::take(&mut self.input);
                let was = std::mem::replace(&mut self.mode, Mode::Normal);
                if was == Mode::AdUri {
                    return self.start_ad(typed);
                }
                self.clamp();
            }
            _ => {}
        }
        None
    }

    fn act_normal(&mut self, action: Action) -> Option<Command> {
        match action {
            Action::Quit => self.quit = true,
            Action::Help => {
                self.mode = if self.mode == Mode::Help { Mode::Normal } else { Mode::Help }
            }
            Action::Cancel => {
                self.mode = Mode::Normal;
                self.filter.clear();
                self.clamp();
            }
            Action::NextPane => self.pane = self.pane.next(),
            Action::PrevPane => self.pane = self.pane.prev(),
            Action::Outputs => self.pane = Pane::Outputs,
            Action::Up => self.move_selection(-1),
            Action::Down => self.move_selection(1),
            Action::Top => self.set_selection(0),
            Action::Bottom => self.set_selection(usize::MAX),
            Action::FilterStart => {
                self.mode = Mode::Filter;
                self.input = self.filter.clone();
            }
            Action::TakeSlot(n) => return self.take_slot(n),
            Action::TakeSelected => {
                let id = self.selected_source().map(|s| s.id.clone());
                return match id {
                    Some(id) => Some(self.take(&id)),
                    None => {
                        self.complain("no source is selected. Add one first, or clear the filter with Escape.");
                        None
                    }
                };
            }
            Action::TakeBlack => {
                self.say("cutting to black");
                return Some(call("program.take", json!({ "source": Value::Null })));
            }
            Action::Revert => return self.revert(),
            Action::Mute => return self.mute(),
            Action::Gain(db) => return self.gain(db),
            Action::AdBreak => return self.adbreak(),
            Action::StartStop => return self.start_stop_output(),
            _ => {}
        }
        None
    }

    fn take_slot(&mut self, n: usize) -> Option<Command> {
        let sources = self.visible_sources();
        match sources.get(n - 1) {
            Some(source) => {
                let id = source.id.clone();
                self.sel_source = n - 1;
                self.pane = Pane::Sources;
                Some(self.take(&id))
            }
            None => {
                self.complain(format!(
                    "there is no source {n} on screen. {} shown{}.",
                    sources.len(),
                    if self.filter.is_empty() {
                        String::new()
                    } else {
                        format!(" under the filter '{}'", self.filter)
                    }
                ));
                None
            }
        }
    }

    fn take(&mut self, id: &str) -> Command {
        self.say(format!("take {id}"));
        call("program.take", json!({ "source": id }))
    }

    fn revert(&mut self) -> Option<Command> {
        if self.revert_missing {
            self.complain(
                "this core has no program.revert. Take the previous source with its number key."
                    .to_string(),
            );
            return None;
        }
        self.say("revert");
        Some(call("program.revert", json!({})))
    }

    fn mute(&mut self) -> Option<Command> {
        let Some(source) = self.selected_source() else {
            self.complain("no source is selected, so there is nothing to mute.");
            return None;
        };
        let (id, muted) = (source.id.clone(), source.muted);
        self.say(format!("{} {id}", if muted { "unmute" } else { "mute" }));
        Some(call("source.audio.set", json!({ "id": id, "muted": !muted })))
    }

    fn gain(&mut self, db: f64) -> Option<Command> {
        let Some(source) = self.selected_source() else {
            self.complain("no source is selected, so there is no fader to move.");
            return None;
        };
        let (id, gain) = (source.id.clone(), source.gain);
        let next = step_gain(gain, db);
        self.say(format!("{id} fader {:+.0} dB", crate::model::gain_to_db(next)));
        Some(call("source.audio.set", json!({ "id": id, "gain": next })))
    }

    fn adbreak(&mut self) -> Option<Command> {
        if self.view().status.ad.is_some() {
            self.say("ending the ad break");
            return Some(call("adbreak.end", json!({})));
        }
        self.mode = Mode::AdUri;
        self.input.clear();
        self.say("the clip to roll, then Enter. Escape gives up.");
        None
    }

    fn start_ad(&mut self, uri: String) -> Option<Command> {
        let uri = uri.trim().to_string();
        if uri.is_empty() {
            self.complain("an ad break needs a clip. Press a again and type a file path or a URI.");
            return None;
        }
        self.say(format!("rolling {uri}"));
        Some(call("adbreak.start", json!({ "uri": uri })))
    }

    /// There is no `output.start` or `output.stop` in api_level 1: an output
    /// exists or it does not. The nearest honest thing is to drop the
    /// connection and make it again, which is what an operator pressing this
    /// on a stuck destination wants.
    fn start_stop_output(&mut self) -> Option<Command> {
        let Some(output) = self.selected_output() else {
            self.complain("no destination is selected. Press o, then pick one with the arrow keys.");
            return None;
        };
        let id = output.id.clone();
        self.say(format!("reconnecting {id} (this build has no output.start or output.stop)"));
        Some(call("output.reconnect", json!({ "id": id })))
    }

    fn move_selection(&mut self, by: isize) {
        let len = self.pane_len();
        if len == 0 {
            return;
        }
        let current = self.selection() as isize;
        let next = (current + by).clamp(0, len as isize - 1) as usize;
        self.set_selection(next);
    }

    fn pane_len(&self) -> usize {
        match self.pane {
            Pane::Sources => self.visible_sources().len(),
            Pane::Outputs => self.visible_outputs().len(),
            Pane::Alerts => self.view().alerts.len(),
        }
    }

    fn selection(&self) -> usize {
        match self.pane {
            Pane::Sources => self.sel_source,
            Pane::Outputs => self.sel_output,
            Pane::Alerts => self.sel_alert,
        }
    }

    fn set_selection(&mut self, to: usize) {
        let len = self.pane_len();
        let to = if len == 0 { 0 } else { to.min(len - 1) };
        match self.pane {
            Pane::Sources => self.sel_source = to,
            Pane::Outputs => self.sel_output = to,
            Pane::Alerts => self.sel_alert = to,
        }
    }

    /// Keep every selection inside its list after the filter or the mixer
    /// changed what is on screen.
    fn clamp(&mut self) {
        self.sel_source = clamp_to(self.sel_source, self.visible_sources().len());
        self.sel_output = clamp_to(self.sel_output, self.visible_outputs().len());
        self.sel_alert = clamp_to(self.sel_alert, self.view().alerts.len());
    }

    /// One message from the link. Answers with a call when the link has to be
    /// told something, which today is only the re-subscribe after a resync.
    pub fn on_incoming(&mut self, incoming: Incoming) -> Option<Command> {
        // Everything except a staged delta is worth a repaint. A staged delta
        // is not: the screen reads `live`, and `live` moves at the flush.
        self.dirty = !matches!(&incoming, Incoming::Event { .. } | Incoming::Frame(_));
        match incoming {
            Incoming::Connected { url } => {
                self.link = Link::Connecting;
                self.say(format!("connected to {url}, waiting for the snapshot"));
            }
            Incoming::Subscribed { seq, ignored_ext } => {
                self.link = Link::Live;
                self.ignored_ext = ignored_ext;
                self.say(format!("subscribed at seq {seq}"));
                if !self.ignored_ext.is_empty() {
                    self.complain(format!(
                        "this core ignored ext {}. Everything else is running.",
                        self.ignored_ext.join(", ")
                    ));
                }
            }
            Incoming::Event { method, params } => {
                let outcome = self.store.apply(&method, &params);
                self.dirty = outcome == Outcome::Render;
                if outcome == Outcome::Resync {
                    let dropped = params.get("dropped").and_then(Value::as_u64).unwrap_or(0);
                    let said = format!(
                        "the mixer says this client fell behind and {dropped} events were dropped. Subscribing again."
                    );
                    self.complain(said.clone());
                    self.store.reset();
                    self.store.note("warning", said);
                    return Some(Command::Resubscribe);
                }
                self.clamp();
            }
            Incoming::Frame(frame) => {
                self.frames_seen += 1;
                self.frame = Some(frame);
                // A new picture is worth a repaint on its own. It arrives at
                // the rate the mosaic was asked for, four a second by default.
                self.dirty = self.wants_picture;
            }
            Incoming::CallOk { method, result } => self.on_result(&method, &result),
            Incoming::CallErr { method, error } => {
                if error.code == -32601 && method == "program.revert" {
                    self.revert_missing = true;
                }
                self.complain(format!("{method}: {} ({})", error.message, error.code));
                self.store.note("error", format!("{method}: {}", error.message));
            }
            Incoming::Disconnected { reason, retry_in } => {
                self.link = Link::Retrying { reason: reason.clone(), in_secs: secs(retry_in) };
                self.complain(format!(
                    "the link to the mixer dropped ({reason}). Trying again in {} s.",
                    secs(retry_in)
                ));
                self.store.note("warning", format!("link dropped: {reason}"));
                self.store.reset();
            }
        }
        None
    }

    fn on_result(&mut self, method: &str, result: &Value) {
        match method {
            "program.take" => {
                let on = result.get("program").and_then(Value::as_str).unwrap_or("black");
                self.say(format!("on air: {on}"));
            }
            "program.revert" => {
                let on = result.get("program").and_then(Value::as_str).unwrap_or("black");
                self.say(format!("reverted, on air: {on}"));
            }
            "source.audio.set" => self.say("fader moved"),
            "output.reconnect" => self.say("the destination is connecting again"),
            "adbreak.start" => self.say("the ad break is armed"),
            "adbreak.end" => self.say("the ad break is over"),
            _ => {}
        }
    }
}

fn clamp_to(index: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        index.min(len - 1)
    }
}

fn secs(d: Duration) -> u64 {
    d.as_millis().div_ceil(1000) as u64
}

fn call(method: &str, params: Value) -> Command {
    Command::Call { method: method.to_string(), params }
}
