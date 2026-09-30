//! What the core adds to a plugin's settings, and where a plugin's events go
//! when the supervisor does not act on them itself.
//!
//! Both exist for the channel server. The core owns channels and their keys,
//! and the ingest plugin's listener needs them; they reach it as one more
//! member of its settings, `channels`, at every start and restart and through
//! `configure` on every change, so there is no window after a restart in which
//! the listener runs without them. The listener's `channel.*` events come back
//! the other way and are handed to whoever registered for them, rather than
//! being read by nothing.

use super::Supervisor;
use crate::config::Params;
use anyhow::{Context, Result};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Called with `(instance, event name, params)` from the supervisor's pump.
/// It must not block: hand the event to a queue and return.
pub type EventRoute = Arc<dyn Fn(&str, &str, &Value) + Send + Sync>;

/// One line a singleton logged, into the core's own log at its level.
pub fn log(instance: &str, level: godwinmix_protocol::plugin::wire::LogLevel, message: &str) {
    use godwinmix_protocol::plugin::wire::LogLevel;
    match level {
        LogLevel::Error => tracing::error!(%instance, "{message}"),
        LogLevel::Warn => tracing::warn!(%instance, "{message}"),
        LogLevel::Info => tracing::info!(%instance, "{message}"),
        LogLevel::Debug | LogLevel::Trace => tracing::debug!(%instance, "{message}"),
    }
}

#[derive(Default)]
pub struct Routes {
    /// `plugin -> key -> value`, laid over the operator's `[plugins.<name>]`.
    extras: Mutex<BTreeMap<String, Params>>,
    /// Event name prefix and where those events go.
    events: Mutex<Vec<(String, EventRoute)>>,
}

impl Routes {
    pub fn add_extras(&self, plugin: &str, params: &mut Params) {
        if let Some(extra) = self.extras.lock().get(plugin) {
            for (k, v) in extra {
                params.insert(k.clone(), v.clone());
            }
        }
    }

    pub fn routes(&self, name: &str) -> bool {
        self.events.lock().iter().any(|(prefix, _)| name.starts_with(prefix.as_str()))
    }

    pub fn deliver(&self, instance: &str, name: &str, params: &Value) {
        let routes: Vec<EventRoute> = self
            .events
            .lock()
            .iter()
            .filter(|(prefix, _)| name.starts_with(prefix.as_str()))
            .map(|(_, route)| route.clone())
            .collect();
        for route in routes {
            route(instance, name, params);
        }
    }
}

impl Supervisor {
    /// Lay `value` over a plugin's settings under `key`, or take it off with
    /// `None`. Held in memory only: it is never written to the config file,
    /// which is what makes it the place for something the core owns.
    pub fn set_extra(&self, plugin: &str, key: &str, value: Option<toml::Value>) {
        let mut extras = self.routes.extras.lock();
        let table = extras.entry(plugin.to_string()).or_default();
        match value {
            Some(v) => {
                table.insert(key.to_string(), v);
            }
            None => {
                table.remove(key);
            }
        }
    }

    /// Send every running instance of a plugin its settings as they now
    /// stand, extras and all. Answers per instance with what `configure`
    /// said. An instance not running yet is skipped: it gets the same
    /// settings when it starts.
    pub fn configure_plugin(&self, plugin: &str) -> Vec<(String, Result<Value>)> {
        let params = crate::plugin::host::source::params_json(&self.params_for(plugin));
        let callers: Vec<(String, crate::plugin::host::Caller)> = {
            let inner = self.inner.lock();
            inner
                .instances
                .iter()
                .filter(|(_, i)| i.plugin == plugin && i.running())
                .filter_map(|(name, i)| i.child.caller().map(|c| (name.clone(), c)))
                .collect()
        };
        callers
            .into_iter()
            .map(|(name, caller)| {
                let answer = caller
                    .call_within(
                        "configure",
                        json!({ "params": params.clone() }),
                        crate::plugin::host::process::CALL_TIMEOUT,
                    )
                    .with_context(|| format!("configuring `{name}`"));
                (name, answer)
            })
            .collect()
    }

    /// Hand events whose name starts with `prefix` to `route` instead of
    /// letting them fall on the floor.
    pub fn route_events(&self, prefix: &str, route: EventRoute) {
        self.routes.events.lock().push((prefix.to_string(), route));
    }

    /// Call a method of one plugin singleton, `<plugin>/<provide>`, with the
    /// protocol's ceiling on the wait. For a method the core itself needs of
    /// a plugin, such as handing the ingest plugin a WHIP offer that arrived
    /// on the control port; tools go through `tool_call`.
    pub fn call_provide(&self, plugin: &str, provide: &str, method: &str, params: Value) -> Result<Value> {
        let wanted = format!("{plugin}/{provide}");
        let caller = {
            let inner = self.inner.lock();
            let instance = inner
                .instances
                .values()
                .find(|i| i.provide == wanted && i.running())
                .ok_or_else(|| anyhow::anyhow!("{wanted} is not running, so `{method}` has nowhere to go"))?;
            instance.child.caller().ok_or_else(|| anyhow::anyhow!("{wanted} has no channel to call on yet"))?
        };
        caller.call_within(method, params, crate::plugin::host::process::CALL_TIMEOUT)
    }

    /// Whether a plugin has a singleton up and answering.
    pub fn is_running(&self, plugin: &str) -> bool {
        self.inner.lock().instances.values().any(|i| i.plugin == plugin && i.running())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extra_is_laid_over_the_operators_settings_and_can_be_taken_off() {
        let supervisor = Supervisor::detached();
        let mut settings = BTreeMap::new();
        let mut ingest = Params::new();
        ingest.insert("rtmp_port".into(), toml::Value::Integer(1936));
        settings.insert("ingest".to_string(), ingest);
        supervisor.set_settings(settings);
        supervisor.set_extra("ingest", "channels", Some(toml::Value::Array(vec![])));
        let params = supervisor.params_for("ingest");
        assert_eq!(params["rtmp_port"].as_integer(), Some(1936));
        assert!(params["channels"].as_array().is_some());
        supervisor.set_extra("ingest", "channels", None);
        assert!(!supervisor.params_for("ingest").contains_key("channels"));
    }

    #[test]
    fn a_routed_event_reaches_its_route_and_no_other() {
        let supervisor = Supervisor::detached();
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = seen.clone();
        supervisor.route_events(
            "channel.",
            Arc::new(move |instance, name, _| log.lock().push(format!("{instance} {name}"))),
        );
        assert!(supervisor.routes.routes("channel.stream"));
        assert!(!supervisor.routes.routes("source.appeared"));
        supervisor.routes.deliver("ingest-discover", "channel.stream", &json!({}));
        assert_eq!(*seen.lock(), vec!["ingest-discover channel.stream".to_string()]);
    }
}
