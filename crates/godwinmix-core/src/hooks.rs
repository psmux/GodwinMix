//! Hooks: lifecycle interception without an in process API (03 section 8).
//!
//! A hook is someone else's code that wants to know a thing happened, and in
//! one case wants a say in it. Borrowed from Claude Code, with the one change
//! a mixer forces: nothing a hook does may reach the encoder.
//!
//! Two different things get called blocking and the difference matters.
//! `take.before` delays the *decision* to take, in the control layer, before
//! the command has reached the pipeline at all. Every other hook is told after
//! the fact and cannot delay anything. In neither case does a hook run on the
//! mixer thread or on a GStreamer streaming thread, so the compositor keeps
//! producing frames on schedule whether or not a decision is pending.
//!
//! What is here is the vocabulary, the configuration, the registry and the
//! policy: which hooks exist, how long one may take, what its answer means.
//! The three transports (`rpc`, `command`, `http`) are in the binary crate,
//! under `control/hooks/`, because two of them need a process and an HTTP
//! client and the engine has neither.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

pub use godwinmix_protocol::plugin::manifest::HOOKS;

/// How long a `take.before` hook gets when it does not say (03 section 8).
/// 20 ms is under one frame at 30 fps, so the take still lands on the frame it
/// was armed for.
pub const DEFAULT_TIMEOUT_MS: u64 = 20;
/// The most a `take.before` hook may ever ask for. Three frames at 30 fps is
/// already a delay an operator can feel; the manifest validator refuses more.
pub const MAX_TIMEOUT_MS: u64 = 100;
/// How long a hook that cannot delay anything is given before it is abandoned.
/// Nothing waits on this; it exists so a wedged webhook does not leave a task
/// and a socket behind for the life of the show.
pub const FIRE_AND_FORGET_TIMEOUT_MS: u64 = 5_000;

/// The hook names this build fires, spelled once.
pub mod name {
    pub const TAKE_BEFORE: &str = "take.before";
    pub const TAKE_AFTER: &str = "take.after";
    pub const SOURCE_ADDED: &str = "source.added";
    pub const SOURCE_REMOVED: &str = "source.removed";
    pub const SOURCE_STATE: &str = "source.state";
    pub const OUTPUT_STATE: &str = "output.state";
    pub const ALERT_RAISED: &str = "alert.raised";
    pub const SESSION_START: &str = "session.start";
    pub const SESSION_END: &str = "session.end";
    pub const PLUGIN_STATE: &str = "plugin.state";
    pub const PLUGIN_LOADED: &str = "plugin.loaded";
    pub const PLUGIN_FAILED: &str = "plugin.failed";
}

/// Whether a hook of this name is allowed to delay the decision behind it.
///
/// Exactly one is, and it says so here rather than in three transports.
pub fn blocks(event: &str) -> bool {
    event == name::TAKE_BEFORE
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// One `[[hooks]]` entry in the operator's config file.
///
/// This is the hook that has no plugin behind it: a URL to POST to, or a
/// command to run, written by whoever runs the mixer rather than by a plugin
/// author. A plugin's own hooks come from `[hooks]` in its manifest and never
/// appear here.
///
/// ```toml
/// [[hooks]]
/// event = "take.after"
/// http = "https://tally.example/on-take"
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HookConfig {
    /// One of [`HOOKS`].
    pub event: String,
    /// `command` mode: a command line, split the way a shell splits one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// `http` mode: a URL to POST the event to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<String>,
    /// `rpc` mode: the plugin to call. Rarely written by hand, because a
    /// plugin normally asks for its own hooks in its manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// `take.before` only, capped at [`MAX_TIMEOUT_MS`]. Ignored elsewhere,
    /// because nothing else waits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// What to call this one in a log line or in `event/hook.blocked`.
    /// Defaults to the target, which is usually the more useful thing anyway.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// How a hook is reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// A JSON-RPC call to a running plugin, through the host.
    Rpc { plugin: String },
    /// A command line. The event arrives as JSON on stdin.
    Command(Vec<String>),
    /// A POST with the event as the body.
    Http(String),
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Rpc { .. } => "rpc",
            Mode::Command(_) => "command",
            Mode::Http(_) => "http",
        }
    }

    /// What the mode points at, for a log line and for `hook.blocked`.
    pub fn target(&self) -> String {
        match self {
            Mode::Rpc { plugin } => plugin.clone(),
            Mode::Command(argv) => argv.first().cloned().unwrap_or_default(),
            Mode::Http(url) => url.clone(),
        }
    }
}

/// One hook, from wherever it was declared.
#[derive(Debug, Clone)]
pub struct Hook {
    /// One of [`HOOKS`].
    pub event: String,
    /// The plugin that asked for it, or `"config"` for a `[[hooks]]` entry.
    /// This is the `plugin` field of `event/hook.blocked`.
    pub owner: String,
    pub mode: Mode,
    /// Only consulted for [`blocks`] hooks.
    pub timeout: Duration,
}

impl Hook {
    /// A name for this hook in a message, which is the owner unless the
    /// operator gave it one.
    pub fn label(&self) -> String {
        if self.owner == CONFIG_OWNER {
            self.mode.target()
        } else {
            self.owner.clone()
        }
    }
}

/// The `owner` of a hook nobody's plugin asked for.
pub const CONFIG_OWNER: &str = "config";

fn timeout_of(event: &str, ms: Option<u64>) -> Duration {
    if !blocks(event) {
        return Duration::from_millis(FIRE_AND_FORGET_TIMEOUT_MS);
    }
    Duration::from_millis(ms.unwrap_or(DEFAULT_TIMEOUT_MS).clamp(1, MAX_TIMEOUT_MS))
}

/// What is wrong with a hook, said the way the error table says it: what the
/// value was and what to write instead.
pub type Problem = String;

impl HookConfig {
    /// Turn one config entry into a hook, or say why not.
    pub fn build(&self, index: usize) -> Result<Hook, Problem> {
        let at = format!("hooks[{index}]");
        if !HOOKS.contains(&self.event.as_str()) {
            return Err(format!(
                "{at}.event: '{}' is not a hook. Known: {}.",
                self.event,
                HOOKS.join(", ")
            ));
        }
        let set = usize::from(self.command.is_some())
            + usize::from(self.http.is_some())
            + usize::from(self.plugin.is_some());
        if set != 1 {
            return Err(format!(
                "{at}: write exactly one of command, http or plugin; {set} were set. A hook \
                 with no plugin behind it is normally `http = \"https://...\"`."
            ));
        }
        let mode = if let Some(url) = &self.http {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(format!(
                    "{at}.http: '{url}' is not an http or https URL."
                ));
            }
            Mode::Http(url.clone())
        } else if let Some(command) = &self.command {
            let argv = shell_words::split(command)
                .map_err(|e| format!("{at}.command: {command} does not parse as a command: {e}"))?;
            if argv.is_empty() {
                return Err(format!("{at}.command: empty."));
            }
            Mode::Command(argv)
        } else {
            Mode::Rpc { plugin: self.plugin.clone().unwrap_or_default() }
        };
        Ok(Hook {
            event: self.event.clone(),
            owner: self.name.clone().unwrap_or_else(|| CONFIG_OWNER.to_string()),
            mode,
            timeout: timeout_of(&self.event, self.timeout_ms),
        })
    }
}

/// Turn a plugin's `[hooks]` table into hooks owned by that plugin.
pub fn from_manifest(
    plugin: &str,
    hooks: &BTreeMap<String, godwinmix_protocol::plugin::manifest::Hook>,
) -> (Vec<Hook>, Vec<Problem>) {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for (event, hook) in hooks {
        if !HOOKS.contains(&event.as_str()) {
            problems.push(format!("{plugin}: '{event}' is not a hook, so it is ignored."));
            continue;
        }
        let mode = match hook.mode.as_str() {
            "rpc" => Mode::Rpc { plugin: plugin.to_string() },
            "http" => match &hook.url {
                Some(url) => Mode::Http(url.clone()),
                None => {
                    problems.push(format!("{plugin}: hook '{event}' is http with no url."));
                    continue;
                }
            },
            "command" => match hook.command.as_deref().map(shell_words::split) {
                Some(Ok(argv)) if !argv.is_empty() => Mode::Command(argv),
                _ => {
                    problems
                        .push(format!("{plugin}: hook '{event}' is command with no command."));
                    continue;
                }
            },
            other => {
                problems.push(format!(
                    "{plugin}: hook '{event}' has mode '{other}'. Known: rpc, command, http."
                ));
                continue;
            }
        };
        out.push(Hook {
            event: event.clone(),
            owner: plugin.to_string(),
            mode,
            timeout: timeout_of(event, hook.timeout_ms.map(u64::from)),
        });
    }
    (out, problems)
}

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

/// Every hook this core will fire, indexed by event.
///
/// A plugin's registrations are reversible: `remove_plugin` unwinds everything
/// it added, which is what `plugin.remove` needs while the show is on air
/// (03 section 7).
#[derive(Debug, Default)]
pub struct Registry {
    by_event: BTreeMap<String, Vec<Hook>>,
}

impl Registry {
    /// The hooks from `[[hooks]]` in the operator's config, with whatever was
    /// wrong with the rest.
    ///
    /// A bad entry is reported and skipped rather than failing the start. A
    /// mixer that will not come up because a webhook URL has a typo in it is
    /// worse than one that comes up and says so.
    pub fn from_config(entries: &[HookConfig]) -> (Self, Vec<Problem>) {
        let mut reg = Registry::default();
        let mut problems = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            match entry.build(index) {
                Ok(hook) => reg.add(hook),
                Err(p) => problems.push(p),
            }
        }
        (reg, problems)
    }

    pub fn add(&mut self, hook: Hook) {
        self.by_event.entry(hook.event.clone()).or_default().push(hook);
    }

    /// Everything a plugin asked for in its manifest.
    pub fn add_plugin(
        &mut self,
        plugin: &str,
        hooks: &BTreeMap<String, godwinmix_protocol::plugin::manifest::Hook>,
    ) -> Vec<Problem> {
        let (built, problems) = from_manifest(plugin, hooks);
        for hook in built {
            self.add(hook);
        }
        problems
    }

    /// Unwind everything one plugin registered.
    pub fn remove_plugin(&mut self, plugin: &str) {
        for hooks in self.by_event.values_mut() {
            hooks.retain(|h| h.owner != plugin);
        }
        self.by_event.retain(|_, hooks| !hooks.is_empty());
    }

    /// The hooks for one event, in the order they were registered.
    pub fn for_event(&self, event: &str) -> &[Hook] {
        self.by_event.get(event).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Whether anything at all is listening for this event. Every call site
    /// asks this first, so a core with no hooks spawns no task and allocates
    /// no payload: nothing runs unless asked.
    pub fn any(&self, event: &str) -> bool {
        self.by_event.get(event).is_some_and(|h| !h.is_empty())
    }

    pub fn is_empty(&self) -> bool {
        self.by_event.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_event.values().map(Vec::len).sum()
    }

    /// One line per hook, for `gmx doctor` and the startup report.
    pub fn describe(&self) -> Vec<String> {
        self.by_event
            .values()
            .flatten()
            .map(|h| {
                let timeout = if blocks(&h.event) {
                    format!(", {} ms", h.timeout.as_millis())
                } else {
                    String::new()
                };
                format!("{} -> {} {}{}", h.event, h.mode.as_str(), h.mode.target(), timeout)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// What a hook answers
// ---------------------------------------------------------------------------

/// What a `take.before` hook said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Refuse { reason: String },
}

impl Decision {
    /// Read a hook's answer.
    ///
    /// Silence is consent: a hook that answers `{}`, or `null`, or anything
    /// without an `allow` key, has not refused. Only an explicit
    /// `{"allow": false}` stops a take, because a hook that refuses by
    /// accident takes a programme off air and the failure mode has to be the
    /// other way round.
    pub fn parse(value: &Value) -> Decision {
        let allow = value.get("allow").and_then(Value::as_bool).unwrap_or(true);
        if allow {
            return Decision::Allow;
        }
        let reason = value
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("the hook refused it and gave no reason")
            .to_string();
        Decision::Refuse { reason }
    }

    /// The exit code convention of 03 section 8: a `command` hook that exits 2
    /// blocks, anything else lets the take through.
    pub fn from_exit(code: Option<i32>, stderr: &str) -> Decision {
        if code != Some(2) {
            return Decision::Allow;
        }
        let said = stderr.trim();
        let reason = if said.is_empty() {
            "the command exited 2, which blocks the take, and printed nothing".to_string()
        } else {
            said.lines().next().unwrap_or(said).to_string()
        };
        Decision::Refuse { reason }
    }

    pub fn is_refusal(&self) -> bool {
        matches!(self, Decision::Refuse { .. })
    }
}

/// Why a hook did not get its say. The payload of `event/hook.blocked`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blocked {
    /// The hook name, for example `take.before`.
    pub hook: String,
    /// The plugin that owns it, or `config`.
    pub plugin: String,
    /// What went wrong, in the same shape as an error message: what happened
    /// and what to do about it.
    pub reason: String,
}

impl Blocked {
    pub fn timed_out(hook: &Hook, waited: Duration) -> Blocked {
        Blocked {
            hook: hook.event.clone(),
            plugin: hook.label(),
            reason: format!(
                "no answer within {} ms, so the take went ahead without it. Raise timeout_ms \
                 (up to {} ms) or move the work off the hook.",
                waited.as_millis(),
                MAX_TIMEOUT_MS
            ),
        }
    }

    pub fn failed(hook: &Hook, why: impl std::fmt::Display) -> Blocked {
        Blocked {
            hook: hook.event.clone(),
            plugin: hook.label(),
            reason: format!("{why}"),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({ "hook": self.hook, "plugin": self.plugin, "reason": self.reason })
    }
}

// ---------------------------------------------------------------------------
// Payloads
// ---------------------------------------------------------------------------

/// The body every hook receives.
///
/// One shape for all three modes, so a `command` hook reading stdin and an
/// `http` receiver reading a POST body parse the same thing. `hook` is in the
/// body as well as in the RPC method name, because a single webhook URL often
/// serves several events.
pub fn envelope(event: &str, payload: Value) -> Value {
    json!({
        "hook": event,
        "ts": crate::observe::logs::rfc3339(&std::time::SystemTime::now()),
        "payload": payload,
    })
}

/// `take.before` and `take.after`: what is being put on air, and who asked.
///
/// `revert` is true when this is `program.revert` rather than
/// `program.take`, because a policy hook usually wants to let an undo through
/// even when it would have refused the take.
pub fn take_payload(
    source: Option<&str>,
    at_running_time_ms: Option<u64>,
    by: &str,
    revert: bool,
) -> Value {
    json!({
        "source": source,
        "at_running_time_ms": at_running_time_ms,
        "by": by,
        "revert": revert,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_protocol::plugin::manifest::Hook as ManifestHook;

    fn cfg(event: &str, http: Option<&str>, command: Option<&str>) -> HookConfig {
        HookConfig {
            event: event.into(),
            http: http.map(str::to_string),
            command: command.map(str::to_string),
            ..HookConfig::default()
        }
    }

    #[test]
    fn a_config_hook_needs_exactly_one_target_and_a_known_event() {
        assert!(cfg("take.after", Some("https://x/y"), None).build(0).is_ok());
        let both = cfg("take.after", Some("https://x/y"), Some("/bin/true")).build(0);
        assert!(both.unwrap_err().contains("exactly one"));
        let none = cfg("take.after", None, None).build(0);
        assert!(none.unwrap_err().contains("exactly one"));
        let unknown = cfg("take.sideways", Some("https://x/y"), None).build(3);
        let message = unknown.unwrap_err();
        // The error names the value and the alternatives, as the error table
        // in 03 section 6 requires of everything.
        assert!(message.contains("hooks[3].event"), "{message}");
        assert!(message.contains("take.before"), "{message}");
    }

    #[test]
    fn a_url_that_is_not_a_url_is_refused_before_the_show_starts() {
        let e = cfg("alert.raised", Some("tally.local/on-take"), None).build(0).unwrap_err();
        assert!(e.contains("is not an http or https URL"), "{e}");
    }

    #[test]
    fn a_command_is_split_the_way_a_shell_splits_one() {
        let hook = cfg("take.after", None, Some("python3 on_take.py --loud 'a b'")).build(0).unwrap();
        assert_eq!(
            hook.mode,
            Mode::Command(vec![
                "python3".into(),
                "on_take.py".into(),
                "--loud".into(),
                "a b".into()
            ])
        );
    }

    #[test]
    fn only_take_before_gets_a_timeout_and_it_is_capped() {
        let mut slow = cfg("take.before", Some("https://x/y"), None);
        slow.timeout_ms = Some(10_000);
        assert_eq!(slow.build(0).unwrap().timeout.as_millis(), MAX_TIMEOUT_MS as u128);

        let default = cfg("take.before", Some("https://x/y"), None).build(0).unwrap();
        assert_eq!(default.timeout.as_millis(), DEFAULT_TIMEOUT_MS as u128);

        let mut after = cfg("take.after", Some("https://x/y"), None);
        after.timeout_ms = Some(10);
        // Nothing waits on take.after, so its timeout is the abandon time, not
        // a delay anyone pays.
        assert_eq!(after.build(0).unwrap().timeout.as_millis(), FIRE_AND_FORGET_TIMEOUT_MS as u128);
        assert!(!blocks("take.after") && blocks("take.before"));
    }

    #[test]
    fn a_bad_entry_is_reported_and_the_rest_still_load() {
        let (reg, problems) = Registry::from_config(&[
            cfg("take.after", Some("https://x/y"), None),
            cfg("nonsense", Some("https://x/y"), None),
            cfg("alert.raised", None, Some("/usr/bin/page-someone")),
        ]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(reg.len(), 2);
        assert!(reg.any("take.after") && reg.any("alert.raised") && !reg.any("take.before"));
    }

    #[test]
    fn a_plugins_registrations_are_unwound_when_it_leaves() {
        let mut reg = Registry::default();
        let hooks = BTreeMap::from([
            ("take.before".to_string(), ManifestHook {
                mode: "rpc".into(),
                timeout_ms: Some(19),
                command: None,
                url: None,
            }),
            ("take.after".to_string(), ManifestHook {
                mode: "rpc".into(),
                timeout_ms: None,
                command: None,
                url: None,
            }),
        ]);
        assert!(reg.add_plugin("compliance", &hooks).is_empty());
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.for_event("take.before")[0].timeout.as_millis(), 19);
        assert_eq!(
            reg.for_event("take.before")[0].mode,
            Mode::Rpc { plugin: "compliance".into() }
        );
        reg.remove_plugin("compliance");
        assert!(reg.is_empty(), "everything the plugin registered goes with it");
    }

    #[test]
    fn silence_is_consent_and_only_an_explicit_refusal_stops_a_take() {
        assert_eq!(Decision::parse(&json!({})), Decision::Allow);
        assert_eq!(Decision::parse(&Value::Null), Decision::Allow);
        assert_eq!(Decision::parse(&json!({"allow": true})), Decision::Allow);
        assert_eq!(
            Decision::parse(&json!({"allow": false, "reason": "cam3 has no audio"})),
            Decision::Refuse { reason: "cam3 has no audio".into() }
        );
        // A refusal with no reason still says something an operator can read.
        let bare = Decision::parse(&json!({"allow": false}));
        assert!(bare.is_refusal());
    }

    #[test]
    fn a_command_hook_blocks_on_exit_two_and_on_nothing_else() {
        assert_eq!(Decision::from_exit(Some(0), ""), Decision::Allow);
        assert_eq!(Decision::from_exit(Some(1), "broken"), Decision::Allow);
        assert_eq!(Decision::from_exit(None, ""), Decision::Allow);
        assert_eq!(
            Decision::from_exit(Some(2), "no audio on cam3\nsecond line"),
            Decision::Refuse { reason: "no audio on cam3".into() }
        );
    }

    #[test]
    fn the_blocked_event_names_the_hook_the_owner_and_the_next_step() {
        let hook = cfg("take.before", Some("https://tally/x"), None).build(0).unwrap();
        let blocked = Blocked::timed_out(&hook, Duration::from_millis(20));
        assert_eq!(blocked.hook, "take.before");
        assert_eq!(blocked.plugin, "https://tally/x");
        assert!(blocked.reason.contains("20 ms"), "{}", blocked.reason);
        assert!(blocked.reason.contains("timeout_ms"), "{}", blocked.reason);
        assert_eq!(blocked.to_json()["hook"], "take.before");
    }

    #[test]
    fn the_envelope_carries_the_hook_name_as_well_as_the_payload() {
        let body = envelope("take.after", take_payload(Some("cam1"), None, "desk", false));
        assert_eq!(body["hook"], "take.after");
        assert_eq!(body["payload"]["source"], "cam1");
        assert_eq!(body["payload"]["by"], "desk");
        assert!(body["ts"].as_str().unwrap().ends_with('Z'));
    }
}
