//! The worker thread: the store, the linker, and the loop that answers jobs.
//!
//! Everything in this file runs on one thread per instance and touches nothing
//! shared except the engine and the memory gauge. That is deliberate: the
//! store is the isolation boundary and it is never behind a lock, so there is
//! no way for a slow component to make anything else wait.

use crate::bindings;
use crate::hostcalls::Callbacks;
use crate::instance::{self, Handshake, Job};
use anyhow::{Context, Result};
use godwinmix_core::plugin::wasm::Spec;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use wasmtime::component::{Component, Linker};
use wasmtime::{ResourceLimiter, Store};
use wasmtime_wasi::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

/// The memory ceiling, and the gauge `plugin.stats` reads.
///
/// wasmtime's own `StoreLimits` enforces the same ceiling but keeps the
/// current size to itself, and a component has no pid for the process sampler
/// to find. So the limiter is written out here: it refuses growth past the
/// cap and remembers what it allowed, which is the only number anybody can
/// report for a component's footprint.
pub struct Limits {
    max_bytes: usize,
    current: usize,
}

impl ResourceLimiter for Limits {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool> {
        if desired > self.max_bytes {
            return Ok(false);
        }
        self.current = desired;
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool> {
        // A table of a million entries is a component doing something other
        // than answering a policy question.
        Ok(desired <= 1_000_000)
    }
}

/// What a store carries beside the component's own memory.
pub struct Ctx {
    pub wasi: WasiCtx,
    pub table: ResourceTable,
    pub calls: Callbacks,
    pub limits: Limits,
}

impl WasiView for Ctx {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl bindings::host::Host for Ctx {
    fn log(&mut self, level: bindings::types::LogLevel, message: String) {
        self.calls.log(level, message)
    }

    fn event(&mut self, name: String, params_json: String) {
        self.calls.event(name, params_json)
    }

    fn core_call(
        &mut self,
        method: String,
        params_json: String,
    ) -> Result<String, bindings::types::Error> {
        self.calls.core_call(method, params_json)
    }

    fn now_ms(&mut self) -> u64 {
        self.calls.now_ms()
    }

    fn running_time_ns(&mut self) -> u64 {
        self.calls.running_time_ns()
    }
}

/// The exports, whichever world this component satisfied.
enum Exports {
    Service(bindings::service::ServicePlugin),
    Transition(bindings::transition::TransitionPlugin),
}

/// One instance, from the file on disk to a thread waiting for jobs.
pub(crate) fn run(
    spec: Spec,
    inbox: mpsc::Receiver<Job>,
    ready: mpsc::Sender<Result<Handshake>>,
    memory: Arc<AtomicU64>,
    spent: Arc<AtomicBool>,
) {
    let name = spec.instance.clone();
    let fuel = spec.grant.fuel_per_call;
    let deadline = spec.grant.deadline;
    match load(spec) {
        Ok((mut store, exports, handshake)) => {
            if ready.send(Ok(handshake)).is_err() {
                // Nobody is waiting: the caller gave up while we compiled.
                return;
            }
            serve(&name, &mut store, &exports, inbox, fuel, deadline, &memory, &spent);
        }
        Err(e) => {
            let _ = ready.send(Err(e));
        }
    }
}

/// Compile, instantiate and hand shake.
fn load(spec: Spec) -> Result<(Store<Ctx>, Exports, Handshake)> {
    let engine = crate::engine::engine()?;
    let bytes = std::fs::read(&spec.component)
        .with_context(|| format!("reading `{}`", spec.component.display()))?;
    let component = Component::new(engine, &bytes).with_context(|| {
        format!(
            "`{}` is not a WebAssembly component. Build it for wasm32-wasip2, which produces \
             a component; a wasm32-wasip1 module is not one and has to be adapted first.",
            spec.component.display()
        )
    })?;
    let mut linker: Linker<Ctx> = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).context("adding WASI to the linker")?;
    bindings::host::add_to_linker::<Ctx, HasSelf>(&mut linker, |ctx| ctx)
        .context("adding the godwinmix:plugin/host interface to the linker")?;
    let mut store = Store::new(engine, context_of(&spec)?);
    store.limiter(|ctx| &mut ctx.limits);
    // The handshake gets the default allowance rather than the instance's per
    // call one. It happens once, it builds whatever state the plugin needs for
    // the rest of its life, and holding it to a budget meant for a hook would
    // make a tight budget mean "this plugin cannot load" rather than "this
    // plugin's hooks are cut short".
    store.set_fuel(spec.grant.fuel_per_call.max(godwinmix_core::plugin::wasm::DEFAULT_FUEL))?;
    store.set_epoch_deadline(START_EPOCHS);
    let hello = hello_of(&spec);
    let (exports, ready) = instantiate(&mut store, &component, &linker, &spec, hello)?;
    Ok((store, exports, ready))
}

/// How many epoch ticks the handshake gets. Twenty seconds, the same wall the
/// caller waits behind; the per call deadline takes over afterwards.
const START_EPOCHS: u64 = 20_000;

/// The projection bindgen wants for a state type that is its own host.
type HasSelf = wasmtime::component::HasSelf<Ctx>;

/// Instantiate under the world the provide's kind asks for, and hand shake.
fn instantiate(
    store: &mut Store<Ctx>,
    component: &Component,
    linker: &Linker<Ctx>,
    spec: &Spec,
    hello: bindings::types::Hello,
) -> Result<(Exports, Handshake)> {
    use godwinmix_core::plugin::ProvideKind;
    let transition = spec.kind == ProvideKind::Transition;
    let exports = if transition {
        Exports::Transition(
            bindings::transition::TransitionPlugin::instantiate(&mut *store, component, linker)
                .with_context(|| {
                    format!(
                        "`{}` is a transition, so its component must export \
                         godwinmix:plugin/transition",
                        spec.provide
                    )
                })?,
        )
    } else {
        Exports::Service(
            bindings::service::ServicePlugin::instantiate(&mut *store, component, linker)
                .with_context(|| {
                    format!(
                        "`{}` is a {}, so its component must export godwinmix:plugin/service",
                        spec.provide,
                        spec.kind.as_str()
                    )
                })?,
        )
    };
    let answer = match &exports {
        Exports::Service(e) => {
            e.godwinmix_plugin_service().call_initialize(&mut *store, &hello)?
        }
        Exports::Transition(e) => {
            e.godwinmix_plugin_transition().call_initialize(&mut *store, &hello)?
        }
    };
    let ready = answer.map_err(|e| instance::from_wit(&e))?;
    Ok((exports, Handshake { tools: ready.tools.clone(), hooks: ready.hooks.clone() }))
}

/// The store's own data: WASI as narrow as the grant allows.
fn context_of(spec: &Spec) -> Result<Ctx> {
    let mut wasi = WasiCtxBuilder::new();
    // stdout and stderr go to the core's log through the plugin's own `log`
    // call, not through a pipe. A component that prints is a component nobody
    // reads, and saying so is better than silently inheriting the core's.
    if spec.grant.filesystem {
        wasi.preopened_dir(
            &spec.root,
            ".",
            wasmtime_wasi::DirPerms::READ,
            wasmtime_wasi::FilePerms::READ,
        )
        .with_context(|| format!("preopening `{}`", spec.root.display()))?;
    }
    if spec.grant.network {
        wasi.inherit_network();
        wasi.allow_ip_name_lookup(true);
    }
    let limits =
        Limits { max_bytes: spec.grant.max_memory_mb as usize * 1024 * 1024, current: 0 };
    Ok(Ctx {
        wasi: wasi.build(),
        table: ResourceTable::new(),
        calls: instance::callbacks(spec),
        limits,
    })
}

fn hello_of(spec: &Spec) -> bindings::types::Hello {
    let fps = spec.canvas.fps;
    bindings::types::Hello {
        core: "godwinmix".into(),
        core_version: env!("CARGO_PKG_VERSION").into(),
        api_level: godwinmix_core::plugin::API_LEVEL,
        api_compatible: godwinmix_core::plugin::API_COMPATIBLE,
        canvas: bindings::types::Canvas {
            width: spec.canvas.width.max(0) as u32,
            height: spec.canvas.height.max(0) as u32,
            fps_n: fps.numer().max(0) as u32,
            fps_d: fps.denom().max(1) as u32,
        },
        instance: spec.instance.clone(),
        provide: format!("{}/{}", spec.plugin, spec.provide),
        params_json: spec.params.to_string(),
        granted: instance::capabilities(&spec.grant),
    }
}

/// Whether an error is the runtime cutting the component off, rather than the
/// component answering with one.
///
/// A trap is fuel, an epoch deadline, a refused allocation, an unreachable, or
/// any of the other ways wasm stops. All of them leave the instance unable to
/// be entered again. An error the plugin itself returned is an ordinary value
/// and leaves it perfectly well.
fn is_trap(e: &anyhow::Error) -> bool {
    e.downcast_ref::<wasmtime::Trap>().is_some()
}

/// Answer jobs until the channel closes, a shutdown arrives, or the component
/// traps and cannot be entered again.
fn serve(
    name: &str,
    store: &mut Store<Ctx>,
    exports: &Exports,
    inbox: mpsc::Receiver<Job>,
    fuel: u64,
    deadline: Duration,
    memory: &Arc<AtomicU64>,
    spent: &Arc<AtomicBool>,
) {
    while let Ok(job) = inbox.recv() {
        match job {
            Job::Shutdown { reason } => {
                let _ = match exports {
                    Exports::Service(e) => {
                        e.godwinmix_plugin_service().call_shutdown(&mut *store, &reason)
                    }
                    Exports::Transition(e) => {
                        e.godwinmix_plugin_transition().call_shutdown(&mut *store, &reason)
                    }
                };
                tracing::debug!(instance = %name, %reason, "a component was shut down");
                return;
            }
            Job::Call { method, params, within, reply } => {
                // Fuel and the epoch are set fresh for every call, so a
                // component that nearly ran out on the last one is not cut
                // early on this one and a component that ran away is not
                // carrying its debt forward either.
                let budget = within.min(deadline).max(Duration::from_millis(1));
                if let Err(e) = store.set_fuel(fuel) {
                    let _ = reply.send(Err(e));
                    continue;
                }
                store.set_epoch_deadline(budget.as_millis() as u64);
                let answer = dispatch(store, exports, &method, params);
                memory.store(used(store), Ordering::Relaxed);
                let trapped = answer.as_ref().err().is_some_and(is_trap);
                let _ = reply.send(answer.with_context(|| format!("`{method}` on `{name}`")));
                if trapped {
                    // A component that traps cannot be entered again: the next
                    // call would answer "cannot enter component instance",
                    // which is a worse error than the one that caused it. So
                    // the worker ends here, the instance reads `failed`, and
                    // the supervisor builds a fresh one on its next pass. The
                    // store is dropped with the thread, which is the whole of
                    // the cleanup a component needs.
                    spent.store(true, Ordering::Relaxed);
                    tracing::warn!(
                        instance = %name, %method,
                        "a component trapped and is spent; it will be started again"
                    );
                    return;
                }
            }
        }
    }
}

/// How much linear memory the component has, for `plugin.stats`.
fn used(store: &mut Store<Ctx>) -> u64 {
    store.data().limits.current as u64
}

/// One method, mapped onto the export of the same name.
fn dispatch(
    store: &mut Store<Ctx>,
    exports: &Exports,
    method: &str,
    params: Value,
) -> Result<Value> {
    match exports {
        Exports::Service(e) => service_call(store, e, method, params),
        Exports::Transition(e) => transition_call(store, e, method, params),
    }
}

fn service_call(
    store: &mut Store<Ctx>,
    e: &bindings::service::ServicePlugin,
    method: &str,
    params: Value,
) -> Result<Value> {
    let api = e.godwinmix_plugin_service();
    match method {
        "configure" => {
            let body = params.get("params").cloned().unwrap_or(params);
            let answer = api
                .call_configure(&mut *store, &body.to_string())?
                .map_err(|e| instance::from_wit(&e))?;
            Ok(json!({
                "applied": answer.applied,
                "restart_required": answer.restart_required,
                "reason": answer.reason,
            }))
        }
        "health" => {
            let h = api.call_health(&mut *store)?;
            Ok(json!({ "state": health_word(h.state), "detail": h.detail, "latency_ms": h.latency_ms }))
        }
        "tool.call" => {
            let name = str_of(&params, "name");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            let text = api
                .call_tool_call(&mut *store, &name, &arguments.to_string())?
                .map_err(|e| instance::from_wit(&e))?;
            parse(&text, "tool.call")
        }
        "hook" => {
            // The core's hook envelope is `{hook, ts, payload}`. A component
            // gets the hook's name as an argument and the whole envelope as
            // the payload, which is what a stdio plugin's `hook` call carries.
            let name = params
                .get("hook")
                .and_then(Value::as_str)
                .unwrap_or_else(|| params.get("name").and_then(Value::as_str).unwrap_or(""))
                .to_string();
            let text = api
                .call_hook(&mut *store, &name, &params.to_string())?
                .map_err(|e| instance::from_wit(&e))?;
            parse(&text, "hook")
        }
        "initialize" | "initialized" => Ok(json!({})),
        other => anyhow::bail!(
            "a service component answers configure, health, tool.call, hook and shutdown; \
             `{other}` is not one of them"
        ),
    }
}

fn transition_call(
    store: &mut Store<Ctx>,
    e: &bindings::transition::TransitionPlugin,
    method: &str,
    params: Value,
) -> Result<Value> {
    let api = e.godwinmix_plugin_transition();
    match method {
        "render" => {
            let answer = api
                .call_render(&mut *store, &params.to_string())?
                .map_err(|e| instance::from_wit(&e))?;
            // The variant is the answer's shape, so the host puts the key on
            // rather than making every author remember it. A component that
            // wrote the key itself is left alone.
            use bindings::transition::exports::godwinmix::plugin::transition::Answer;
            match answer {
                Answer::Pads(t) => under("pads", parse(&t, "render")?),
                Answer::Curve(t) => under("curve", parse(&t, "render")?),
            }
        }
        "configure" => {
            let body = params.get("params").cloned().unwrap_or(params);
            let answer = api
                .call_configure(&mut *store, &body.to_string())?
                .map_err(|e| instance::from_wit(&e))?;
            Ok(json!({
                "applied": answer.applied,
                "restart_required": answer.restart_required,
                "reason": answer.reason,
            }))
        }
        "health" => {
            let h = api.call_health(&mut *store)?;
            Ok(json!({ "state": health_word(h.state), "detail": h.detail, "latency_ms": h.latency_ms }))
        }
        "initialize" | "initialized" => Ok(json!({})),
        other => anyhow::bail!(
            "a transition component answers render, configure, health and shutdown; \
             `{other}` is not one of them"
        ),
    }
}

/// Put the answer under its key, unless the component already did.
fn under(key: &str, answer: Value) -> Result<Value> {
    if answer.get(key).is_some() {
        return Ok(answer);
    }
    Ok(json!({ key: answer }))
}

fn health_word(state: bindings::types::HealthState) -> &'static str {
    match state {
        bindings::types::HealthState::Ok => "ok",
        bindings::types::HealthState::Degraded => "degraded",
        bindings::types::HealthState::Failing => "failing",
    }
}

fn parse(text: &str, method: &str) -> Result<Value> {
    serde_json::from_str(text).with_context(|| {
        format!("the component's `{method}` answer was not JSON: {}", first_line(text))
    })
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").chars().take(120).collect()
}

fn str_of(params: &Value, key: &str) -> String {
    params.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}
