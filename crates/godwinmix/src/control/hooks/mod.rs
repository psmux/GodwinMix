//! Firing hooks: the three transports and the one rule they share.
//!
//! The vocabulary and the policy are in `godwinmix_core::hooks`. This is what
//! actually reaches out: a JSON-RPC call to a plugin (`rpc`), a command with
//! the event on its stdin (`command`), or a POST (`http`).
//!
//! The rule they share is that nothing here runs on the mixer thread or on a
//! GStreamer streaming thread. Every hook runs as a Tokio task. `take.before`
//! is awaited, in the control layer, before the take command has reached the
//! pipeline at all; everything else is spawned and forgotten. So the worst a
//! hook can do to the programme is delay one operator's decision by its own
//! `timeout_ms`, and the compositor keeps producing frames either way.
//!
//! Nothing runs unless asked: [`Hooks::fire`] looks the event up first and
//! returns without allocating a payload when no hook wants it, which is the
//! case on every core nobody has configured one on.

pub mod command;
pub mod http;
pub mod rpc;

use godwinmix_core::hooks::{blocks, envelope, Blocked, Decision, Hook, Mode, Registry};
use godwinmix_protocol::types::Event;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

/// Where a hook's own event goes. `event/hook.blocked` is published through
/// this, which on a running core is the mixer's event bus.
pub type Emitter = Arc<dyn Fn(Event) + Send + Sync>;

/// Every hook this core will fire, and the things they need to reach.
pub struct Hooks {
    registry: RwLock<Registry>,
    /// One client for every `http` hook, so a webhook fired on every take
    /// reuses its connection rather than shaking hands each time.
    http: reqwest::Client,
    /// Where `event/hook.blocked` goes. A closure rather than a handle so a
    /// test can watch what was published without building a mixer.
    events: Emitter,
    /// Long lived plugin processes for `rpc` mode. See `rpc.rs`.
    sidecars: rpc::Sidecars,
}

impl Hooks {
    /// The hooks from `[[hooks]]` in the operator's config, plus whatever the
    /// installed plugins asked for in their manifests.
    ///
    /// Anything wrong with an entry is logged and skipped. A mixer that will
    /// not start because a webhook URL has a typo in it is worse than one that
    /// starts and says so.
    pub fn new(config: &[godwinmix_core::hooks::HookConfig], events: Emitter) -> Arc<Hooks> {
        let (mut registry, problems) = Registry::from_config(config);
        for problem in &problems {
            tracing::warn!("{problem}");
        }
        for installed in godwinmix_core::plugin::loader::enabled() {
            let name = installed.name().to_string();
            for problem in registry.add_plugin(&name, &installed.manifest.hooks) {
                tracing::warn!("{problem}");
            }
        }
        if !registry.is_empty() {
            tracing::info!(hooks = registry.len(), "hooks: {}", registry.describe().join("; "));
        }
        Arc::new(Hooks {
            registry: RwLock::new(registry),
            http: reqwest::Client::builder()
                .user_agent(concat!("godwinmix/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
            events,
            sidecars: rpc::Sidecars::default(),
        })
    }

    /// A core with no hooks at all, which is what every test and every embedded
    /// core has until somebody configures one.
    pub fn detached(events: Emitter) -> Arc<Hooks> {
        Hooks::new(&[], events)
    }

    /// Take on everything one plugin asked for. Called after `plugin.add`.
    pub fn register_plugin(
        &self,
        name: &str,
        hooks: &std::collections::BTreeMap<String, godwinmix_protocol::plugin::manifest::Hook>,
    ) {
        for problem in self.registry.write().add_plugin(name, hooks) {
            tracing::warn!("{problem}");
        }
    }

    /// Unwind everything one plugin registered, and stop its hook process.
    /// Called after `plugin.remove`, because registrations are reversible.
    pub fn unregister_plugin(&self, name: &str) {
        self.registry.write().remove_plugin(name);
        self.sidecars.drop_plugin(name);
    }

    /// Whether anything is listening. Every call site asks this first.
    pub fn any(&self, event: &str) -> bool {
        self.registry.read().any(event)
    }

    /// One line per hook, for `gmx doctor`.
    pub fn describe(&self) -> Vec<String> {
        self.registry.read().describe()
    }

    /// Tell whoever asked that a thing happened, and do not wait.
    ///
    /// The one call site shape for every hook that cannot delay a decision:
    /// `hooks.fire(hooks::name::SOURCE_ADDED, json!({ .. }))`. The payload
    /// closure is only run when somebody is listening.
    pub fn fire(self: &Arc<Self>, event: &'static str, payload: impl FnOnce() -> Value) {
        let hooks: Vec<Hook> = {
            let reg = self.registry.read();
            if !reg.any(event) {
                return;
            }
            reg.for_event(event).to_vec()
        };
        let body = envelope(event, payload());
        for hook in hooks {
            let me = self.clone();
            let body = body.clone();
            tokio::spawn(async move {
                if let Err(e) = me.run(&hook, body).await {
                    // Nothing waited on this, so the only thing to do with a
                    // failure is say so, with enough in it to fix.
                    me.announce_blocked(Blocked::failed(&hook, format!("{e:#}")));
                }
            });
        }
    }

    /// Ask every `take.before` hook, in parallel, and answer within the
    /// longest timeout any of them asked for.
    ///
    /// Returns the first refusal. A hook that does not answer in time does not
    /// delay the take: its task is abandoned, `event/hook.blocked` says so,
    /// and the take goes ahead. That is the whole of the acceptance line in 07
    /// Phase 6, and the reason the wait is here in the control layer rather
    /// than anywhere near the pipeline.
    pub async fn take_before(self: &Arc<Self>, payload: impl FnOnce() -> Value) -> Option<String> {
        let event = godwinmix_core::hooks::name::TAKE_BEFORE;
        let hooks: Vec<Hook> = {
            let reg = self.registry.read();
            if !reg.any(event) {
                return None;
            }
            reg.for_event(event).to_vec()
        };
        let body = envelope(event, payload());
        let mut running = Vec::new();
        for hook in hooks {
            let me = self.clone();
            let body = body.clone();
            let timeout = hook.timeout;
            running.push(tokio::spawn(async move {
                let started = Instant::now();
                let answer = tokio::time::timeout(timeout, me.run(&hook, body)).await;
                (hook, started.elapsed(), answer)
            }));
        }
        let mut refusal = None;
        for task in running {
            let Ok((hook, took, answer)) = task.await else { continue };
            match answer {
                Err(_) => self.announce_blocked(Blocked::timed_out(&hook, took)),
                Ok(Err(e)) => self.announce_blocked(Blocked::failed(&hook, format!("{e:#}"))),
                Ok(Ok(Decision::Allow)) => {}
                Ok(Ok(Decision::Refuse { reason })) if refusal.is_none() => {
                    refusal = Some(format!("{}: {reason}", hook.label()));
                }
                Ok(Ok(Decision::Refuse { .. })) => {}
            }
        }
        refusal
    }

    /// One hook, whichever mode it is in.
    ///
    /// The answer only means anything for a hook that [`blocks`]; for every
    /// other one it is read so a broken target is reported, and thrown away.
    async fn run(&self, hook: &Hook, body: Value) -> anyhow::Result<Decision> {
        match &hook.mode {
            Mode::Http(url) => http::post(&self.http, url, hook, body).await,
            Mode::Command(argv) => command::run(argv, hook, body).await,
            Mode::Rpc { plugin } => self.sidecars.call(plugin, hook, body).await,
        }
    }

    /// `event/hook.blocked`, and a warning in the log beside it.
    fn announce_blocked(&self, blocked: Blocked) {
        tracing::warn!(
            hook = %blocked.hook,
            plugin = %blocked.plugin,
            "a hook did not get its say: {}",
            blocked.reason
        );
        (self.events)(Event::HookBlocked {
            hook: blocked.hook,
            plugin: blocked.plugin,
            reason: blocked.reason,
        });
    }
}

/// Whether this hook name is one that delays a decision. Re-exported so a call
/// site does not have to reach into the engine crate for it.
pub use godwinmix_core::hooks::name;

/// True when the event given is the blocking one. Used by the tests.
pub fn is_blocking(event: &str) -> bool {
    blocks(event)
}

// ---------------------------------------------------------------------------
// The one background task: everything the event stream already says
// ---------------------------------------------------------------------------

/// Turn the events the core already publishes into hooks.
///
/// `source.state`, `output.state`, `alert.raised` and `plugin.state` are all
/// things the event stream carries, so one subscriber fires them all rather
/// than four call sites scattered through the engine. The task exits at once
/// on a core with no hooks configured for any of them.
pub fn spawn_watch(hooks: Arc<Hooks>, mut events: tokio::sync::broadcast::Receiver<godwinmix_core::state::Envelope>) {
    const WATCHED: &[&str] =
        &[name::SOURCE_STATE, name::OUTPUT_STATE, name::ALERT_RAISED, name::PLUGIN_STATE];
    if !WATCHED.iter().any(|e| hooks.any(e)) {
        return;
    }
    tokio::spawn(async move {
        loop {
            let envelope = match events.recv().await {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            };
            match envelope.event {
                Event::SourceStateChanged { source, state } => {
                    hooks.fire(name::SOURCE_STATE, || {
                        serde_json::json!({ "source": source, "state": state })
                    });
                }
                Event::OutputStateChanged { output, state, reconnects } => {
                    hooks.fire(name::OUTPUT_STATE, || {
                        serde_json::json!({
                            "output": output, "state": state, "reconnects": reconnects
                        })
                    });
                }
                Event::Alert { severity, message } => {
                    hooks.fire(name::ALERT_RAISED, || {
                        serde_json::json!({ "severity": severity, "message": message })
                    });
                }
                // A hook fired on its own blocking is how a loop starts.
                Event::HookBlocked { .. } => {}
                _ => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_core::hooks::HookConfig;

    /// An emitter that keeps what was published, so a test can read it.
    fn recorder() -> (Emitter, Arc<parking_lot::Mutex<Vec<Event>>>) {
        let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let mine = seen.clone();
        (Arc::new(move |e| mine.lock().push(e)), seen)
    }

    #[tokio::test]
    async fn a_core_with_no_hooks_fires_nothing_and_allows_every_take() {
        let hooks = Hooks::detached(recorder().0);
        assert!(!hooks.any(name::TAKE_BEFORE));
        // The payload closure is never run, which is what keeps a core with no
        // hooks free of the cost of having them.
        let mut built = false;
        let refusal = hooks
            .take_before(|| {
                built = true;
                Value::Null
            })
            .await;
        assert!(refusal.is_none());
        assert!(!built, "the payload should not be built when nothing is listening");
    }

    #[test]
    fn the_registry_is_described_for_gmx_doctor() {
        let hooks = Hooks::new(
            &[HookConfig {
                event: "take.after".into(),
                http: Some("https://tally.example/on-take".into()),
                ..HookConfig::default()
            }],
            recorder().0,
        );
        let lines = hooks.describe();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("take.after -> http https://tally.example/on-take"), "{lines:?}");
    }
}
