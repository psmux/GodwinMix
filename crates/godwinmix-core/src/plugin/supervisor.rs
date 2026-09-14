//! The supervisor: every plugin instance that is not a source.
//!
//! A source instance is created by an operator, one per `source.add`, and the
//! mixer owns it because it owns the pipeline it feeds. The other three kinds
//! the core hosts are not like that. There is one mDNS browser per plugin, not
//! one per camera; one OSC bridge, not one per message; one wipe, not one per
//! take. So they are singletons, named after their provide (`ndi-discovery`),
//! started with the core and kept up by this.
//!
//! ```text
//!   loader (what is installed)            mixer (pipelines)
//!         |                                     ^
//!         | provides_of_kind                    | Command::AddSource
//!         v                                     |
//!   +-----------------------------------------+ |
//!   | supervisor                              |-+
//!   |  one SidecarService per singleton       |
//!   |  a pump thread: notices, restarts       |
//!   |  tool.call, discover, render            |
//!   +-----------------------------------------+
//! ```
//!
//! # Why a thread and not the mixer
//!
//! Because the mixer thread has commands to answer and a plugin call can take
//! five seconds. Nothing in here touches a pipeline: the one thing it asks the
//! mixer for is `AddSource` and `RemoveSource`, through the same queue every
//! other caller uses, so a plugin that goes mad cannot do anything an operator
//! could not do by hand.
//!
//! # What the pump does, every 250 ms
//!
//! 1. Drains each instance's notices. A device's `event/source.appeared`
//!    becomes a source; `event/source.gone` takes it away again. A publisher
//!    connecting to an ingest plugin is therefore live inside a quarter of a
//!    second, well inside the five seconds the roadmap asks for.
//! 2. Answers requests a plugin made of the core, so a plugin that asks
//!    something the core will not do gets an error rather than a hang.
//! 3. Restarts an instance that has died, under the same backoff a source
//!    gets: three free, then 30 seconds doubling to 300, cleared on a start
//!    that works.

use crate::caps::CanvasCaps;
use crate::config::{Params, SourceConfig};
use crate::mixer::{transition, Command, MixerHandle};
use crate::plugin::host::{SidecarService, SidecarSpec};
use crate::plugin::{loader, ProvideKind};
use anyhow::{Context, Result};
use godwinmix_host::lifecycle::Backoff;
use godwinmix_protocol::plugin::wire::{Candidate, InstanceState};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// How often the pump looks at every instance.
///
/// A quarter of a second: twenty times inside the five seconds a publisher has
/// to become a live source, and cheap enough that an idle core with three
/// plugins does nothing measurable. Each pass is a lock, a drain of an
/// already filled queue, and nothing else.
pub const PUMP: Duration = Duration::from_millis(250);

/// What a device says when something it was listening for turns up.
pub const APPEARED: &str = "source.appeared";
/// And when it goes away again.
pub const GONE: &str = "source.gone";

/// One singleton instance and the state the supervisor keeps beside it.
struct Instance {
    kind: ProvideKind,
    plugin: String,
    /// `<plugin>/<provide>`, which is what `launch_for` is given.
    provide: String,
    child: SidecarService,
    backoff: Backoff,
    /// The moment before which the next start attempt must not happen.
    not_before: Option<Instant>,
}

impl Instance {
    fn running(&self) -> bool {
        !matches!(self.child.instance_state(), InstanceState::Failed | InstanceState::Stopped)
    }
}

/// One tier W singleton: a component running inside this process.
///
/// Held apart from the process instances rather than inside them, because
/// almost nothing the supervisor does to a process applies: there is no pid to
/// sample, no pipe to drain, no exit to notice and no backoff to serve. What
/// is left is the call, and the call is the same call.
struct Component {
    kind: ProvideKind,
    plugin: String,
    provide: String,
    /// An `Arc` so a caller clones it out and drops the table lock before the
    /// call. A component call can take its whole deadline and the table is
    /// read by `tool.call`, by `plugin.list` and by every take.
    instance: Arc<dyn crate::plugin::wasm::Instance>,
}

struct Inner {
    instances: BTreeMap<String, Instance>,
    /// Tier W singletons by instance name, `min-hold-hold`.
    components: BTreeMap<String, Component>,
    /// Sources added because a device said they appeared, with the instance
    /// that said so. Only these are taken away again: a device may not remove
    /// a camera an operator added by hand.
    adopted: BTreeMap<String, String>,
}

/// Everything that is not a source, kept running.
pub struct Supervisor {
    inner: Mutex<Inner>,
    canvas: CanvasCaps,
    /// `[plugins.<name>]` from the operator's config, by plugin name.
    settings: Mutex<BTreeMap<String, Params>>,
    mixer: Mutex<Option<MixerHandle>>,
    stopping: AtomicBool,
}

impl Supervisor {
    pub fn new(canvas: CanvasCaps, settings: BTreeMap<String, Params>) -> Arc<Supervisor> {
        Arc::new(Supervisor {
            inner: Mutex::new(Inner {
                instances: BTreeMap::new(),
                components: BTreeMap::new(),
                adopted: BTreeMap::new(),
            }),
            canvas,
            settings: Mutex::new(settings),
            mixer: Mutex::new(None),
            stopping: AtomicBool::new(false),
        })
    }

    /// A supervisor with nothing to supervise, for a core with no plugins and
    /// for a test that only wants the shape.
    pub fn detached() -> Arc<Supervisor> {
        Supervisor::new(CanvasCaps::new(&Default::default()), BTreeMap::new())
    }

    /// Where an auto added source goes. Without this the supervisor still runs
    /// services and answers tools; a device simply has nowhere to put what it
    /// finds and says so in the log.
    pub fn attach(&self, mixer: MixerHandle) {
        *self.mixer.lock() = Some(mixer);
    }

    pub fn set_settings(&self, settings: BTreeMap<String, Params>) {
        *self.settings.lock() = settings;
    }

    fn params_for(&self, plugin: &str) -> Params {
        self.settings.lock().get(plugin).cloned().unwrap_or_default()
    }

    // -- starting and stopping ------------------------------------------

    /// Start every singleton of every plugin that is installed and enabled.
    ///
    /// Called once at startup and again after `plugin.add`, so the second is
    /// the first with most of the work already done: an instance that is
    /// already running is left alone.
    pub fn start_all(&self) -> Vec<(String, String)> {
        let mut failures = Vec::new();
        for kind in [ProvideKind::Service, ProvideKind::Device, ProvideKind::Transition] {
            for provide in loader::provides_of_kind(kind.as_str()) {
                if let Err(e) = self.start(&provide) {
                    warn!(%provide, ?e, "a plugin singleton would not start");
                    failures.push((provide, format!("{e:#}")));
                }
            }
        }
        failures
    }

    /// Start one provide, by `<plugin>/<id>`. Already running is success.
    pub fn start(&self, provide: &str) -> Result<()> {
        let manifest = loader::provide_manifest(provide)
            .with_context(|| format!("no provide called `{provide}` in any loaded plugin"))?;
        if !manifest.kind.is_singleton() {
            anyhow::bail!(
                "`{provide}` is a {} and is not run as a singleton. A source is created by \
                 `source.add`, an output by `output.add`, a filter by `filter.add`.",
                manifest.kind.as_str()
            );
        }
        let instance = format!("{}-{}", manifest.plugin, manifest.id);
        if self.inner.lock().instances.get(&instance).is_some_and(Instance::running) {
            return Ok(());
        }
        // Tier W: the plugin runs inside this process rather than beside it.
        // Decided by the manifest and the operator's `place`, never here.
        if let Some(installed) = loader::get(manifest.plugin) {
            if crate::plugin::wasm::runs_as_wasm(&installed.manifest) {
                return self.start_component(provide, manifest, &installed, &instance);
            }
        }
        let params = self.params_for(manifest.plugin);
        let child = self.build(provide, manifest)?;
        let mut entry = Instance {
            kind: manifest.kind,
            plugin: manifest.plugin.to_string(),
            provide: provide.to_string(),
            child,
            backoff: Backoff::new(),
            not_before: None,
        };
        entry.child.start(&self.canvas, &params).with_context(|| {
            format!("starting the {} `{instance}`", manifest.kind.as_str())
        })?;
        info!(%instance, kind = manifest.kind.as_str(), "plugin singleton started");
        self.inner.lock().instances.insert(instance, entry);
        Ok(())
    }

    /// Build the sidecar for one provide without starting it.
    fn build(&self, provide: &str, manifest: &'static crate::plugin::Manifest) -> Result<SidecarService> {
        Ok(SidecarService::new(self.spec(provide, manifest)?))
    }

    /// The launch plan for one provide: the command line, the environment, the
    /// token and the runtime directory.
    fn spec(
        &self,
        provide: &str,
        manifest: &'static crate::plugin::Manifest,
    ) -> Result<SidecarSpec> {
        let instance = format!("{}-{}", manifest.plugin, manifest.id);
        let launched = loader::launch_for(
            provide,
            &instance,
            loader::mint_token(provide, &instance),
            loader::rpc_url(),
        )?;
        Ok(SidecarSpec {
            plugin: launched.plugin,
            provide: launched.provide,
            manifest: *manifest,
            launch: launched.launch,
            ctx: launched.ctx,
            canvas: self.canvas.clone(),
            runtime: loader::runtime_dir(),
        })
    }

    /// Stop every singleton of one plugin and forget what they adopted.
    pub fn stop_plugin(&self, plugin: &str, reason: &str) -> usize {
        let components = self.stop_components(plugin, reason);
        let taken: Vec<(String, Instance)> = {
            let mut inner = self.inner.lock();
            let names: Vec<String> = inner
                .instances
                .iter()
                .filter(|(_, i)| i.plugin == plugin)
                .map(|(k, _)| k.clone())
                .collect();
            names
                .into_iter()
                .filter_map(|k| inner.instances.remove(&k).map(|i| (k, i)))
                .collect()
        };
        let stopped = taken.len();
        for (name, mut instance) in taken {
            instance.child.stop(reason);
            self.disown(&name);
            debug!(instance = %name, reason, "plugin singleton stopped");
        }
        stopped + components
    }

    /// Everything, at shutdown.
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let plugins: Vec<String> = {
            let inner = self.inner.lock();
            let mut names: Vec<String> =
                inner.instances.values().map(|i| i.plugin.clone()).collect();
            names.extend(inner.components.values().map(|c| c.plugin.clone()));
            names.sort();
            names.dedup();
            names
        };
        for plugin in plugins {
            self.stop_plugin(&plugin, "the core is shutting down");
        }
    }

    /// Take every source a device added away again.
    fn disown(&self, instance: &str) {
        let sources: Vec<String> = {
            let inner = self.inner.lock();
            inner
                .adopted
                .iter()
                .filter(|(_, by)| by.as_str() == instance)
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in sources {
            self.remove_source(&id);
        }
    }

    // -- what the control plane asks for --------------------------------

    /// Every singleton, as `(instance, plugin, provide, state)`.
    pub fn instances(&self) -> Vec<(String, String, String, String)> {
        let mut rows: Vec<(String, String, String, String)> = self
            .inner
            .lock()
            .instances
            .iter()
            .map(|(name, i)| {
                (
                    name.clone(),
                    i.plugin.clone(),
                    i.provide.clone(),
                    i.child.instance_state().as_str().to_string(),
                )
            })
            .collect();
        rows.extend(self.component_rows());
        rows.sort();
        rows
    }

    /// The transitions a plugin has added to the built in four.
    pub fn transition_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .inner
            .lock()
            .instances
            .values()
            .filter(|i| i.kind == ProvideKind::Transition)
            .map(|i| i.plugin.clone())
            .collect();
        names.extend(self.component_transition_names());
        names.sort();
        names.dedup();
        names
    }

    /// Call one of a plugin's `[[tools]]`, in MCP's shape.
    ///
    /// `name` is `<plugin>/<tool>`, which is what the MCP layer sends and what
    /// `plugin.describe` prints. A bare tool name is accepted when exactly one
    /// plugin has it, because that is what a person types.
    pub fn tool_call(&self, name: &str, arguments: Value) -> Result<Value> {
        // `<plugin>/<tool>`, `<tool>` when only one plugin has it, or
        // `<plugin>/<provide>/<tool>` when a plugin has two instances that
        // both answer and the caller means a particular one.
        let parts: Vec<&str> = name.split('/').collect();
        let (plugin, provide, tool) = match parts.as_slice() {
            [tool] => (None, None, tool.to_string()),
            [plugin, tool] => (Some(plugin.to_string()), None, tool.to_string()),
            [plugin, provide, tool] => {
                (Some(plugin.to_string()), Some(provide.to_string()), tool.to_string())
            }
            _ => {
                anyhow::bail!(
                    "`{name}` is not a tool name. Use `<plugin>/<tool>`, or \
                     `<plugin>/<provide>/<tool>` to pick one of a plugin's instances."
                )
            }
        };
        // A component answers the same `tool.call` a process does, so it is
        // looked at first and by the same name.
        if let Some(component) = self.component_with_tool(plugin.as_deref(), &tool) {
            return component.call("tool.call", json!({ "name": tool, "arguments": arguments }));
        }
        if let Some(provide) = provide {
            let instance = format!("{}-{provide}", plugin.clone().unwrap_or_default());
            let inner = self.inner.lock();
            let entry = inner.instances.get(&instance).with_context(|| {
                format!(
                    "no instance called `{instance}`. Running now: {}",
                    inner.instances.keys().cloned().collect::<Vec<_>>().join(", ")
                )
            })?;
            return entry
                .child
                .call("tool.call", json!({ "name": tool, "arguments": arguments }));
        }
        let inner = self.inner.lock();
        // `[[tools]]` are declared once per plugin, not per provide, so every
        // one of a plugin's instances answers to the same list. The service is
        // the one that gets the call: a tool is control plane work and a
        // service is the control plane kind. One instance per plugin, so a
        // plugin with a service and a device is not "two plugins offer this".
        let mut found: Vec<&Instance> = Vec::new();
        for instance in inner.instances.values() {
            if !plugin.as_deref().is_none_or(|p| instance.plugin == p) {
                continue;
            }
            if !instance.child.tools().contains(&tool) {
                continue;
            }
            match found.iter().position(|i| i.plugin == instance.plugin) {
                Some(at) if rank(instance.kind) < rank(found[at].kind) => found[at] = instance,
                Some(_) => {}
                None => found.push(instance),
            }
        }
        match found.as_slice() {
            [one] => one.child.call("tool.call", json!({ "name": tool, "arguments": arguments })),
            [] => {
                let known = self.tool_names_locked(&inner);
                anyhow::bail!(
                    "no tool called `{name}`. Running now: {}. `plugin.list` says which \
                     plugins are up and `plugin.describe` says what each one offers.",
                    if known.is_empty() { "none".into() } else { known.join(", ") }
                )
            }
            many => anyhow::bail!(
                "`{tool}` is offered by {} plugins: {}. Name it as `<plugin>/{tool}`.",
                many.len(),
                many.iter().map(|i| i.plugin.as_str()).collect::<Vec<_>>().join(", ")
            ),
        }
    }

    fn tool_names_locked(&self, inner: &Inner) -> Vec<String> {
        inner
            .instances
            .values()
            .flat_map(|i| i.child.tools().into_iter().map(move |t| format!("{}/{t}", i.plugin)))
            .collect()
    }

    /// Ask every device what is out there and merge the answers.
    ///
    /// Sequential on purpose. Devices are few, each one is bounded by
    /// `DISCOVER_TIMEOUT`, and a caller that wants them in parallel can call
    /// `discover` once per device.
    pub fn discover(&self, timeout: Duration) -> Vec<Candidate> {
        let inner = self.inner.lock();
        let devices: Vec<&Instance> =
            inner.instances.values().filter(|i| i.kind == ProvideKind::Device).collect();
        let each = match devices.len() {
            0 => return Vec::new(),
            n => timeout / n as u32,
        };
        let mut found = Vec::new();
        for device in devices {
            match device.child.discover(each) {
                Ok(candidates) => found.extend(candidates),
                Err(e) => warn!(instance = %device.child.instance(), ?e, "a device would not discover"),
            }
        }
        found
    }

    // -- the pump -------------------------------------------------------

    /// Run the pump on a thread of its own until `shutdown`.
    pub fn spawn_pump(self: &Arc<Self>) {
        let me = self.clone();
        let started = std::thread::Builder::new()
            .name("plugin-supervisor".into())
            .spawn(move || {
                while !me.stopping.load(Ordering::SeqCst) {
                    me.pump();
                    std::thread::sleep(PUMP);
                }
            });
        if let Err(e) = started {
            warn!(?e, "the plugin supervisor's pump would not start; plugins will not be watched");
        }
    }

    /// One pass: notices, requests, restarts.
    pub fn pump(&self) {
        let work: Vec<(String, ProvideKind, Vec<crate::plugin::host::Notice>)> = {
            let inner = self.inner.lock();
            inner
                .instances
                .iter()
                .map(|(name, i)| (name.clone(), i.kind, i.child.notices()))
                .collect()
        };
        for (instance, kind, notices) in work {
            for notice in notices {
                self.absorb(&instance, kind, notice);
            }
        }
        self.sample_components();
        self.restart_the_dead();
    }

    /// One thing a plugin said.
    fn absorb(&self, instance: &str, kind: ProvideKind, notice: crate::plugin::host::Notice) {
        use crate::plugin::host::Notice;
        match notice {
            Notice::Event { name, params } if kind == ProvideKind::Device => {
                self.device_event(instance, &name, &params)
            }
            Notice::Event { name, .. } => {
                debug!(%instance, %name, "a plugin raised an event the core does not route")
            }
            Notice::Request { id, method, params } => self.answer(instance, &id, &method, params),
            Notice::HealthChanged { state, detail } => {
                loader::set_state(instance, &state);
                debug!(%instance, %state, ?detail, "a plugin's health changed");
            }
            Notice::Broken(why) => warn!(%instance, %why, "a plugin's channel broke"),
            Notice::Log { .. } | Notice::MediaReport(_) => {}
        }
    }

    /// A device saying something turned up or went away.
    ///
    /// Two spellings are read, because two are in use and neither is wrong.
    /// The core's own vocabulary is `source.appeared` and `source.gone`. A
    /// plugin that names its event after itself (`ingest.publisher`) says which
    /// it means in an `action` field, which is the shape an event stream takes
    /// when one event carries both halves of a lifecycle. Anything else is
    /// logged and ignored: a device is free to raise events of its own that
    /// the core has no business acting on.
    fn device_event(&self, instance: &str, name: &str, params: &Value) {
        let action = params.get("action").and_then(Value::as_str).unwrap_or("");
        let arrived = name == APPEARED || matches!(action, "connected" | "appeared" | "added");
        let left = name == GONE || matches!(action, "left" | "gone" | "disconnected" | "removed");
        if arrived {
            match serde_json::from_value::<Candidate>(params.clone()) {
                Ok(candidate) => self.adopt(instance, &candidate, named_id(params)),
                Err(e) => warn!(
                    %instance, %name, ?e,
                    "a device said something arrived but did not describe it: an arrival \
                     needs type, name and params"
                ),
            }
            return;
        }
        if left {
            match named_id(params).or_else(|| {
                params.get("name").and_then(Value::as_str).map(slug)
            }) {
                Some(id) if self.inner.lock().adopted.contains_key(&id) => self.remove_source(&id),
                Some(id) => debug!(%instance, %id, "a device let go of a source it did not add"),
                None => warn!(%instance, %name, "a device said something left but not what"),
            }
            return;
        }
        debug!(%instance, %name, "a device raised an event the core does not route");
    }

    /// Add what a device found as a source.
    fn adopt(&self, instance: &str, candidate: &Candidate, named: Option<String>) {
        // The id the device chose, when it chose one. A device that relays a
        // publisher already has a name for it and the source it expects to be
        // created; inventing a different one here would mean its own tools
        // could not find what it asked for.
        let id = match named {
            Some(id) => id,
            None => self.free_id(&slug(&candidate.name)),
        };
        let uri = candidate
            .params
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut cfg = SourceConfig::bare(&id, &uri);
        cfg.name = Some(candidate.name.clone());
        if !candidate.kind.is_empty() {
            cfg.type_id = Some(candidate.kind.clone());
        }
        if let Some(table) = candidate.params.as_object() {
            for (key, value) in table {
                if key == "uri" {
                    continue;
                }
                if let Ok(v) = toml::Value::try_from(value) {
                    cfg.params.insert(key.clone(), v);
                }
            }
        }
        let Some(mixer) = self.mixer.lock().clone() else {
            warn!(%instance, %id, "a device found a source but this core has no mixer to put it on");
            return;
        };
        match mixer.send(Command::AddSource(Box::new(cfg), None)) {
            Ok(()) => {
                self.inner.lock().adopted.insert(id.clone(), instance.to_string());
                info!(%instance, %id, name = %candidate.name, "a device added a source");
            }
            Err(e) => warn!(%instance, %id, ?e, "the mixer would not take a device's source"),
        }
    }

    fn remove_source(&self, id: &str) {
        self.inner.lock().adopted.remove(id);
        let Some(mixer) = self.mixer.lock().clone() else { return };
        match mixer.send(Command::RemoveSource(id.to_string(), None)) {
            Ok(()) => info!(%id, "a device let go of a source"),
            Err(e) => warn!(%id, ?e, "the mixer would not let go of a device's source"),
        }
    }

    /// An id nothing has taken. A device that finds the same camera twice gets
    /// `cam` and `cam-2`, which is what `source.add` does for a person.
    fn free_id(&self, base: &str) -> String {
        let inner = self.inner.lock();
        let base = if base.is_empty() { "device" } else { base };
        if !inner.adopted.contains_key(base) {
            return base.to_string();
        }
        (2..99)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !inner.adopted.contains_key(id))
            .unwrap_or_else(|| format!("{base}-{}", inner.adopted.len() + 1))
    }

    /// Answer a request a plugin made of the core.
    ///
    /// Two methods, because those are the two a device needs and because a
    /// plugin that wants the rest of the protocol has `GMX_RPC` and a token
    /// for it. Anything else is refused by name rather than left hanging,
    /// which is the difference between a plugin that logs an error and one
    /// that stops answering `health`.
    fn answer(&self, instance: &str, id: &Value, method: &str, params: Value) {
        let result = match method {
            "source.add" => match serde_json::from_value::<Candidate>(params.clone()) {
                Ok(candidate) => {
                    let named = named_id(&params);
                    self.adopt(instance, &candidate, named);
                    Ok(json!({"added": true}))
                }
                Err(e) => Err(format!(
                    "source.add over the plugin channel takes a candidate with type, name \
                     and params: {e}"
                )),
            },
            "source.remove" => {
                match params.get("id").and_then(Value::as_str) {
                    Some(id) => {
                        self.remove_source(&slug(id));
                        Ok(json!({"removed": true}))
                    }
                    None => Err("source.remove takes an id".to_string()),
                }
            }
            other => Err(format!(
                "the plugin channel answers `source.add` and `source.remove` and nothing \
                 else; `{other}` goes to the core's /rpc, whose URL is in GMX_RPC and whose \
                 token is in GMX_TOKEN"
            )),
        };
        let inner = self.inner.lock();
        let Some(entry) = inner.instances.get(instance) else { return };
        let answer = match result {
            Ok(value) => entry.child.answer(id, value),
            Err(message) => {
                warn!(%instance, %method, %message, "refused a plugin's request");
                entry.child.answer(id, json!({"error": message}))
            }
        };
        if let Err(e) = answer {
            debug!(%instance, ?e, "could not answer a plugin's request");
        }
    }

    /// Start again anything that has died, under the backoff.
    fn restart_the_dead(&self) {
        let dead: Vec<String> = {
            let inner = self.inner.lock();
            inner
                .instances
                .iter()
                .filter(|(_, i)| !i.running())
                .filter(|(_, i)| i.not_before.is_none_or(|at| Instant::now() >= at))
                .map(|(name, _)| name.clone())
                .collect()
        };
        for name in dead {
            let (provide, plugin) = {
                let inner = self.inner.lock();
                let Some(entry) = inner.instances.get(&name) else { continue };
                (entry.provide.clone(), entry.plugin.clone())
            };
            let params = self.params_for(&plugin);
            let manifest = loader::provide_manifest(&provide);
            let built = manifest.map(|m| self.build(&provide, m));
            let mut inner = self.inner.lock();
            let Some(entry) = inner.instances.get_mut(&name) else { continue };
            let wait = entry.backoff.next_wait();
            entry.not_before = Some(Instant::now() + wait);
            if wait > Duration::ZERO {
                warn!(instance = %name, wait_secs = wait.as_secs(), "waiting before starting a plugin again");
                continue;
            }
            match built {
                Some(Ok(child)) => {
                    entry.child = child;
                    match entry.child.start(&self.canvas, &params) {
                        Ok(()) => {
                            entry.backoff.clear();
                            entry.not_before = None;
                            info!(instance = %name, "a plugin singleton was started again");
                        }
                        Err(e) => warn!(instance = %name, ?e, "starting a plugin again did not work"),
                    }
                }
                Some(Err(e)) => warn!(instance = %name, ?e, "could not build a plugin again"),
                None => {
                    // The plugin was removed under us. Nothing to start.
                    inner.instances.remove(&name);
                }
            }
        }
    }

    // -- reload ---------------------------------------------------------

    /// Swap a plugin's running instances for fresh ones, one at a time.
    ///
    /// One at a time so that a plugin with a service and a device keeps the
    /// other one answering while the first is replaced, and so that a failure
    /// stops at the instance that failed rather than after everything is
    /// already down. A new instance that will not hand shake is thrown away
    /// and the previous one is started again from the previous launch plan,
    /// which is what makes a bad reload a no change rather than an outage.
    ///
    /// The picture is covered throughout: a singleton draws nothing, and a
    /// source belonging to the same plugin keeps its slot and its last frame
    /// because the mixer's own freeze frame covers a source being rebuilt.
    pub fn reload(&self, plugin: &str) -> Reloaded {
        let names: Vec<String> = {
            let inner = self.inner.lock();
            inner
                .instances
                .iter()
                .filter(|(_, i)| i.plugin == plugin)
                .map(|(k, _)| k.clone())
                .collect()
        };
        let mut report = Reloaded::default();
        self.reload_components(plugin, &mut report);
        for name in names {
            let provide = {
                let inner = self.inner.lock();
                match inner.instances.get(&name) {
                    Some(entry) => entry.provide.clone(),
                    None => continue,
                }
            };
            match self.swap(&name, &provide, plugin) {
                Ok(()) => report.swapped.push(name),
                Err(e) => report.failed.push((name, e.to_string())),
            }
        }
        report
    }

    /// Replace one instance, rolling back to the previous one on a failure.
    fn swap(&self, instance: &str, provide: &str, plugin: &str) -> Result<()> {
        let manifest = loader::provide_manifest(provide)
            .with_context(|| format!("`{provide}` is no longer in any loaded plugin"))?;
        let params = self.params_for(plugin);
        // Built before anything is stopped, so a plugin whose new version will
        // not even launch never takes the old one down.
        let mut fresh = self.build(provide, manifest)?;
        let previous = {
            let mut inner = self.inner.lock();
            inner.instances.remove(instance)
        };
        if let Some(old) = previous {
            // Taken apart rather than assigned over. Both instances carry the
            // same name, and a `SidecarService` clears that name's rows when it
            // goes; the old value has to be gone before the new one registers,
            // or the running singleton ends up invisible to `plugin.list` and
            // to the budget sampler.
            let Instance { plugin: owner, provide: launch, .. } = &old;
            let (owner, launch) = (owner.clone(), launch.clone());
            drop(old);
            match fresh.start(&self.canvas, &params) {
                Ok(()) => {
                    self.inner.lock().instances.insert(
                        instance.to_string(),
                        Instance {
                            kind: manifest.kind,
                            plugin: owner,
                            provide: launch,
                            child: fresh,
                            backoff: Backoff::new(),
                            not_before: None,
                        },
                    );
                    info!(%instance, "swapped under plugin.reload");
                    return Ok(());
                }
                Err(e) => {
                    // The new one would not shake hands. Build the previous
                    // version's launch plan again and start it, so a bad
                    // reload is a no change rather than an outage.
                    let rolled = self
                        .build(&launch, manifest)
                        .and_then(|mut back| back.start(&self.canvas, &params).map(|()| back));
                    let (child, note) = match rolled {
                        Ok(back) => (back, None),
                        Err(also) => (self.build(&launch, manifest)?, Some(also)),
                    };
                    match &note {
                        None => warn!(%instance, ?e, "the new instance failed; the previous one is back"),
                        Some(also) => warn!(
                            %instance, ?e, ?also,
                            "the new instance failed and the old one would not come back; the \
                             pump will keep trying under the backoff"
                        ),
                    }
                    self.inner.lock().instances.insert(
                        instance.to_string(),
                        Instance {
                            kind: manifest.kind,
                            plugin: owner,
                            provide: launch,
                            child,
                            backoff: Backoff::new(),
                            not_before: None,
                        },
                    );
                    return Err(e);
                }
            }
        }
        // Nothing was running: this is a start, not a swap.
        fresh.start(&self.canvas, &params)?;
        self.inner.lock().instances.insert(
            instance.to_string(),
            Instance {
                kind: manifest.kind,
                plugin: plugin.to_string(),
                provide: provide.to_string(),
                child: fresh,
                backoff: Backoff::new(),
                not_before: None,
            },
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tier W
// ---------------------------------------------------------------------------

/// Everything the `wasm` placement adds to the supervisor.
///
/// It is all here rather than threaded through the process path because the
/// two have almost nothing in common below the call: a component has no pid,
/// no pipe and no exit code, so the pump has nothing to pump and the backoff
/// has nothing to back off from. What they share is that `tool.call`, `hook`
/// and `render` mean the same thing at both placements, and those three are
/// the only places the rest of the core has to know both exist.
impl Supervisor {
    /// Bring a component up as a singleton, the way a process singleton is.
    fn start_component(
        &self,
        provide: &str,
        manifest: &'static crate::plugin::Manifest,
        installed: &loader::Installed,
        instance: &str,
    ) -> Result<()> {
        use crate::plugin::wasm;
        if self.inner.lock().components.contains_key(instance) {
            return Ok(());
        }
        let file = installed
            .manifest
            .run
            .as_ref()
            .and_then(|r| r.wasm.as_ref())
            .context("the manifest declares the `wasm` placement but [run] wasm names no file")?;
        let component = installed.root.join(file);
        anyhow::ensure!(
            component.is_file(),
            "`{}` names `{file}` as its component and there is no such file under `{}`. \
             Build it with `cargo build --release --target wasm32-wasip2` and copy the \
             `.wasm` into the plugin directory.",
            manifest.plugin,
            installed.root.display()
        );
        let spec = wasm::Spec {
            plugin: manifest.plugin.to_string(),
            provide: manifest.id.to_string(),
            instance: instance.to_string(),
            kind: manifest.kind,
            component,
            root: installed.root.clone(),
            canvas: self.canvas.clone(),
            params: params_value(&self.params_for(manifest.plugin)),
            grant: self.grant_for(installed),
        };
        let running = wasm::start(spec)
            .with_context(|| format!("starting the component `{instance}`"))?;
        loader::set_hosted(instance, manifest.plugin, provide, running.memory_bytes());
        loader::set_state(instance, "ready");
        wasm::publish(manifest.plugin, running.clone());
        self.inner.lock().components.insert(
            instance.to_string(),
            Component {
                kind: manifest.kind,
                plugin: manifest.plugin.to_string(),
                provide: provide.to_string(),
                instance: running,
            },
        );
        info!(%instance, kind = manifest.kind.as_str(), "plugin component started");
        Ok(())
    }

    /// What this instance is allowed to do.
    ///
    /// The manifest asks and the operator allows; neither alone is enough for
    /// WASI. The memory ceiling is `[plugins.<name>] max_rss_mb`, the same key
    /// a process is held to, so an operator sets one number per plugin
    /// whichever placement it runs at.
    fn grant_for(&self, installed: &loader::Installed) -> crate::plugin::wasm::Grant {
        use crate::plugin::wasm::{wasi_allowed, Grant, DEFAULT_DEADLINE, DEFAULT_FUEL};
        let asks = &installed.manifest.plugin.wasi;
        let allowed = wasi_allowed(installed.name());
        let wants = |what: &str| allowed && asks.iter().any(|a| a == what);
        // `wasm_fuel` and `wasm_deadline_ms` are the two limits an operator can
        // move. Both are ceilings: lowering one makes a slow component fail
        // sooner, and neither can make it run longer than the caller waits.
        let settings = self.params_for(installed.name());
        let number = |key: &str| settings.get(key).and_then(toml::Value::as_integer);
        Grant {
            fuel_per_call: number("wasm_fuel").map(|v| v.max(0) as u64).unwrap_or(DEFAULT_FUEL),
            deadline: number("wasm_deadline_ms")
                .map(|v| Duration::from_millis(v.clamp(1, 5_000) as u64))
                .unwrap_or(DEFAULT_DEADLINE),
            filesystem: wants("filesystem"),
            network: wants("network"),
            // A service that enforces a policy on takes has to be able to see
            // the programme, and a director has to be able to change it. The
            // narrow list is inside the host function; this only says whether
            // `program.take` is in it, and it follows the hooks the plugin
            // asked for: a plugin that never sees a take does not get to make
            // one.
            take: installed.manifest.hooks.keys().any(|h| h.starts_with("take.")),
            max_memory_mb: installed.budget.max_rss_mb.unwrap_or(64).min(u32::MAX as u64) as u32,
            ..Grant::default()
        }
    }

    /// The component of one plugin, cloned out with no lock held.
    ///
    /// Every caller must go through this. A component call runs to its
    /// deadline, and a lock held across one would stall `plugin.list` and
    /// every other take behind an unrelated plugin.
    fn component_of(
        &self,
        plugin: &str,
        kind: ProvideKind,
    ) -> Option<Arc<dyn crate::plugin::wasm::Instance>> {
        let inner = self.inner.lock();
        inner
            .components
            .values()
            .find(|c| c.kind == kind && c.plugin == plugin)
            .map(|c| c.instance.clone())
    }

    /// The component that answers one tool, cloned out the same way.
    fn component_with_tool(
        &self,
        plugin: Option<&str>,
        tool: &str,
    ) -> Option<Arc<dyn crate::plugin::wasm::Instance>> {
        let inner = self.inner.lock();
        inner
            .components
            .values()
            .find(|c| {
                plugin.is_none_or(|p| c.plugin == p) && c.instance.tools().iter().any(|t| t == tool)
            })
            .map(|c| c.instance.clone())
    }

    /// Every component, in the shape `instances()` uses.
    fn component_rows(&self) -> Vec<(String, String, String, String)> {
        let taken: Vec<(String, ComponentRow)> = {
            let inner = self.inner.lock();
            inner
                .components
                .iter()
                .map(|(name, c)| {
                    (
                        name.clone(),
                        ComponentRow {
                            plugin: c.plugin.clone(),
                            provide: c.provide.clone(),
                            instance: c.instance.clone(),
                        },
                    )
                })
                .collect()
        };
        // `state()` is on the component, so it is read after the lock is
        // dropped like everything else that crosses the boundary.
        taken
            .into_iter()
            .map(|(name, c)| {
                (name, c.plugin, c.provide, c.instance.state().as_str().to_string())
            })
            .collect()
    }

    /// What each component costs, once a pass, for `plugin.list`.
    ///
    /// Read out from under the lock like every other component call: asking a
    /// component its memory is cheap, but nothing in here holds the table over
    /// a call into one.
    fn sample_components(&self) {
        let rows: Vec<(String, String, String, Arc<dyn crate::plugin::wasm::Instance>)> = {
            let inner = self.inner.lock();
            inner
                .components
                .iter()
                .map(|(name, c)| {
                    (name.clone(), c.plugin.clone(), c.provide.clone(), c.instance.clone())
                })
                .collect()
        };
        for (name, plugin, provide, instance) in rows {
            loader::set_hosted(&name, &plugin, &provide, instance.memory_bytes());
            loader::set_state(&name, instance.state().as_str());
        }
    }

    /// Plugin names with a transition component up.
    fn component_transition_names(&self) -> Vec<String> {
        let inner = self.inner.lock();
        inner
            .components
            .values()
            .filter(|c| c.kind == ProvideKind::Transition)
            .map(|c| c.plugin.clone())
            .collect()
    }

    /// Take one plugin's components down. The counterpart of `stop_plugin`.
    fn stop_components(&self, plugin: &str, reason: &str) -> usize {
        let taken: Vec<(String, Component)> = {
            let mut inner = self.inner.lock();
            let names: Vec<String> = inner
                .components
                .iter()
                .filter(|(_, c)| c.plugin == plugin)
                .map(|(k, _)| k.clone())
                .collect();
            names.into_iter().filter_map(|k| inner.components.remove(&k).map(|c| (k, c))).collect()
        };
        let stopped = taken.len();
        for (name, component) in taken {
            component.instance.shutdown(reason);
            loader::set_state(&name, "stopped");
            loader::forget(&name);
            debug!(instance = %name, reason, "plugin component stopped");
        }
        if stopped > 0 {
            crate::plugin::wasm::retire(plugin);
        }
        stopped
    }

    /// Recompile and swap one plugin's components. Hot reload for tier W is
    /// stopping the old store and building a new one from the file on disk,
    /// which is the whole of it: there is no process to outlive the swap.
    fn reload_components(&self, plugin: &str, report: &mut Reloaded) {
        let provides: Vec<(String, String)> = {
            let inner = self.inner.lock();
            inner
                .components
                .iter()
                .filter(|(_, c)| c.plugin == plugin)
                .map(|(name, c)| (name.clone(), c.provide.clone()))
                .collect()
        };
        if provides.is_empty() {
            return;
        }
        self.stop_components(plugin, "plugin.reload");
        for (name, provide) in provides {
            match self.start(&provide) {
                Ok(()) => report.swapped.push(name),
                Err(e) => report.failed.push((name, format!("{e:#}"))),
            }
        }
    }
}

/// The three fields `component_rows` carries out from under the lock, so the
/// component's own `state()` is read with nothing held.
struct ComponentRow {
    plugin: String,
    provide: String,
    instance: Arc<dyn crate::plugin::wasm::Instance>,
}

/// `[plugins.<name>]` as the JSON a plugin's `params` is.
fn params_value(params: &Params) -> Value {
    serde_json::to_value(params).unwrap_or(Value::Null)
}

/// What one `plugin.reload` did.
#[derive(Debug, Clone, Default)]
pub struct Reloaded {
    pub swapped: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl transition::Renderer for Supervisor {
    fn render(&self, plugin: &str, request: &transition::RenderRequest) -> Result<Value> {
        let params = serde_json::to_value(request).context("encoding a render request")?;
        // Tier W first, and with the table lock dropped before the call: a
        // component runs to its own fuel or deadline, and nothing else in the
        // supervisor may wait behind it.
        if let Some(component) = self.component_of(plugin, ProvideKind::Transition) {
            return component.call_within("render", params, RENDER_DEADLINE);
        }
        let inner = self.inner.lock();
        let entry = inner
            .instances
            .values()
            .find(|i| i.kind == ProvideKind::Transition && i.plugin == plugin)
            .with_context(|| {
                format!(
                    "no transition plugin called `{plugin}` is running. Installed and \
                     enabled transitions: {}",
                    match self.transition_names_locked(&inner).join(", ") {
                        s if s.is_empty() => "none".to_string(),
                        s => s,
                    }
                )
            })?;
        entry.child.call_within("render", params, RENDER_DEADLINE)
    }
}

impl Supervisor {
    fn transition_names_locked(&self, inner: &Inner) -> Vec<String> {
        inner
            .instances
            .values()
            .filter(|i| i.kind == ProvideKind::Transition)
            .map(|i| i.plugin.clone())
            .collect()
    }
}

/// How long one `render` call may take.
///
/// The whole sampling of a transition is budgeted in the mixer; this is the
/// per call deadline inside it, so one slow answer cannot eat the lot.
const RENDER_DEADLINE: Duration = Duration::from_millis(50);

/// Which instance of a plugin answers a tool call, best first.
fn rank(kind: ProvideKind) -> u8 {
    match kind {
        ProvideKind::Service => 0,
        ProvideKind::Device => 1,
        _ => 2,
    }
}

/// The id a device named for something it found, if it named one.
fn named_id(params: &Value) -> Option<String> {
    let id = params.get("id").and_then(Value::as_str)?;
    let id = slug(id);
    (!id.is_empty()).then_some(id)
}

/// A name a person typed, as an id. The same rule `source.add` applies, so a
/// device finding "CAM 1 (Studio)" produces `cam-1-studio`.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_a_person_typed_becomes_an_id_a_command_can_carry() {
        assert_eq!(slug("CAM 1 (Studio)"), "cam-1-studio");
        assert_eq!(slug("  ndi://host/Cam  "), "ndi-host-cam");
        assert_eq!(slug("---"), "");
    }

    #[test]
    fn a_supervisor_with_nothing_in_it_answers_rather_than_panicking() {
        let s = Supervisor::detached();
        assert!(s.instances().is_empty());
        assert!(s.transition_names().is_empty());
        assert!(s.discover(Duration::from_millis(10)).is_empty());
        let e = s.tool_call("nope", json!({})).expect_err("no tools are running");
        assert!(e.to_string().contains("no tool called"), "{e}");
        assert!(e.to_string().contains("plugin.list"), "the message must name the next step");
        let reloaded = s.reload("nothing");
        assert!(reloaded.swapped.is_empty() && reloaded.failed.is_empty());
        s.shutdown();
    }
}
