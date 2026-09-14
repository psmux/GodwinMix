//! Tier W: the seam a WebAssembly component is plugged into.
//!
//! The engine does not carry wasmtime. This module is the shape of a tier W
//! instance and a registry of one runner; `godwinmix-wasm` is the runner, and
//! the binary registers it at startup when it was built with `--features
//! wasm`. With the feature off the registry is empty and a `wasm` placement is
//! refused with a message that names the build flag, which is a better answer
//! than a plugin that silently does nothing.
//!
//! ```text
//!   supervisor ---- start(spec) ----> Runner (godwinmix-wasm, optional)
//!        |                                |
//!        |  Arc<dyn Instance>             |  one wasmtime Store per instance
//!        v                                v
//!   call("hook", ..)  call("render", ..)  the component
//! ```
//!
//! # Why the calls are JSON
//!
//! Because a tier W plugin is the same plugin as a tier 2 one with a different
//! door. `call("hook", params)` carries the bytes a stdio plugin would have
//! written on its line, and the answer comes back the same way, so the
//! supervisor, the hook dispatcher and the transition renderer all treat the
//! two placements identically. The WIT world spells out the same methods:
//! `docs/reference/wasm.md` has the table.
//!
//! # What never crosses
//!
//! Media. There is no buffer, no pad and no caps in any signature here. A
//! `source`, `output`, `filter` or `encoder` provide asked to run as `wasm` is
//! refused with -32005 and the message names the placements that do carry
//! media. See `docs/explanation/why-wasm-is-not-on-the-frame-path.md`.

use crate::caps::CanvasCaps;
use anyhow::Result;
use godwinmix_protocol::plugin::manifest::{Manifest as PluginManifest, WASM_KINDS};
use godwinmix_protocol::plugin::wire::InstanceState;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// How long one call into a component may take before the host cuts it.
///
/// A hook has its own, shorter, deadline from `[hooks]`; this is the ceiling
/// for everything else and it is deliberately below the protocol's five second
/// call limit.
pub const DEFAULT_DEADLINE: Duration = Duration::from_millis(500);

/// The per call fuel allowance.
///
/// Fuel is counted in executed wasm operations. Half a billion is about a
/// tenth of a second of straight line arithmetic on the reference laptop, far
/// more than a policy decision or a curve needs and far less than a runaway
/// loop gets through before the deadline would have caught it. The number is
/// a ceiling, not a budget to spend: a component that hits it is cut.
pub const DEFAULT_FUEL: u64 = 500_000_000;

/// What the host granted one instance. The plugin reads it in the handshake.
#[derive(Debug, Clone)]
pub struct Grant {
    pub core_call: bool,
    pub take: bool,
    pub events: bool,
    pub filesystem: bool,
    pub network: bool,
    pub fuel_per_call: u64,
    pub deadline: Duration,
    pub max_memory_mb: u32,
}

impl Default for Grant {
    /// What a plugin gets when the manifest asks for nothing and the operator
    /// allows nothing: it may log, raise events and read the core, and it has
    /// no filesystem, no sockets and 64 MiB.
    fn default() -> Self {
        Self {
            core_call: true,
            take: false,
            events: true,
            filesystem: false,
            network: false,
            fuel_per_call: DEFAULT_FUEL,
            deadline: DEFAULT_DEADLINE,
            max_memory_mb: 64,
        }
    }
}

/// Everything a runner needs to bring one component up.
#[derive(Debug, Clone)]
pub struct Spec {
    /// The plugin name, `min-hold`.
    pub plugin: String,
    /// The provide id within the plugin, `hold`.
    pub provide: String,
    /// The instance name the supervisor knows it by, `min-hold-hold`.
    pub instance: String,
    /// The provide's kind. Decides which of the two worlds is instantiated.
    pub kind: super::ProvideKind,
    /// The component file, absolute.
    pub component: PathBuf,
    /// The plugin's own directory, for a WASI preopen when one is granted.
    pub root: PathBuf,
    pub canvas: CanvasCaps,
    /// The validated `params` object.
    pub params: Value,
    pub grant: Grant,
}

/// One running component, as everything above it sees it.
///
/// Every method is the JSON-RPC method of the same name in 03 section 6. A
/// caller that has a `SidecarService` and one of these in hand can treat them
/// the same way, and the supervisor does.
pub trait Instance: Send + Sync {
    /// One call, with the instance's own deadline.
    fn call(&self, method: &str, params: Value) -> Result<Value>;

    /// One call with a deadline of its own, shorter or longer. A hook uses its
    /// `timeout_ms`; a `render` uses the transition renderer's.
    fn call_within(&self, method: &str, params: Value, within: Duration) -> Result<Value>;

    /// `[[tools]]` this instance answers, unprefixed.
    fn tools(&self) -> Vec<String>;

    /// Hooks it asked for, `take.before` and the rest.
    fn hooks(&self) -> Vec<String>;

    fn state(&self) -> InstanceState;

    /// Resident bytes of the component's linear memory, for `plugin.stats`.
    /// A component has no pid, so the sampler cannot find this any other way.
    fn memory_bytes(&self) -> u64;

    fn shutdown(&self, reason: &str);
}

/// What turns a `Spec` into an `Instance`. Implemented once, in
/// `godwinmix-wasm`.
pub trait Runner: Send + Sync {
    fn start(&self, spec: Spec) -> Result<Arc<dyn Instance>>;

    /// What the build carries, for `gmx doctor`: the engine and its version.
    fn describe(&self) -> String;
}

static RUNNER: OnceLock<Arc<dyn Runner>> = OnceLock::new();

/// Install the runner. Called once, by the binary, when it was built with the
/// `wasm` feature. A second call is ignored: the first runner wins and the
/// core never swaps an engine under a running component.
pub fn register(runner: Arc<dyn Runner>) {
    if RUNNER.set(runner).is_err() {
        tracing::debug!("a second WASM runner was offered and ignored");
    }
}

/// Whether this build can run a component at all.
pub fn present() -> bool {
    RUNNER.get().is_some()
}

/// One line for `gmx doctor`.
pub fn describe() -> Option<String> {
    RUNNER.get().map(|r| r.describe())
}

/// Bring a component up, or say why this build cannot.
pub fn start(spec: Spec) -> Result<Arc<dyn Instance>> {
    let runner = RUNNER.get().ok_or_else(|| {
        anyhow::anyhow!(
            "`{}` wants the `wasm` placement and this build does not carry the WebAssembly \
             host. Rebuild with `cargo build --release --features wasm`, or install the \
             plugin at a placement this build has: {}.",
            spec.plugin,
            "in-process, sidecar, node"
        )
    })?;
    runner.start(spec)
}

// ---------------------------------------------------------------------------
// The live table
// ---------------------------------------------------------------------------

/// Running components by plugin name.
///
/// The supervisor owns the instances; this is the index the hook dispatcher
/// reaches them through, because hooks are fired from the control layer and a
/// hook on a tier W plugin must reach the singleton that is already up rather
/// than start a second copy of it (which is what `rpc` mode does for a
/// process).
static LIVE: Mutex<Option<BTreeMap<String, Arc<dyn Instance>>>> = Mutex::new(None);

pub fn publish(plugin: &str, instance: Arc<dyn Instance>) {
    LIVE.lock().get_or_insert_with(BTreeMap::new).insert(plugin.to_string(), instance);
}

pub fn retire(plugin: &str) {
    if let Some(table) = LIVE.lock().as_mut() {
        table.remove(plugin);
    }
}

/// The running component of one plugin, cloned out so no lock is held across
/// the call. Every caller must use it this way: a component call can take its
/// whole deadline, and a lock held over it would stall the table for anything
/// else that wants it.
pub fn live(plugin: &str) -> Option<Arc<dyn Instance>> {
    LIVE.lock().as_ref()?.get(plugin).cloned()
}

/// Plugin names with a component up, for `plugin.list` and for a message that
/// names the alternatives.
pub fn running() -> Vec<String> {
    LIVE.lock().as_ref().map(|t| t.keys().cloned().collect()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Which placement a provide runs at
// ---------------------------------------------------------------------------

/// What the operator wrote: `[plugins.<name>]` per plugin, and the
/// `[plugins] allow_wasi` list.
#[derive(Debug, Default)]
struct Operator {
    settings: BTreeMap<String, toml::Table>,
    allow_wasi: Vec<String>,
}

static OPERATOR: Mutex<Option<Operator>> = Mutex::new(None);

/// Told once at startup, and again after a config reload. Without it every
/// plugin is read as if the operator had written nothing, which is the right
/// answer for an embedded core and for every test.
pub fn set_config(settings: BTreeMap<String, toml::Table>, allow_wasi: Vec<String>) {
    *OPERATOR.lock() = Some(Operator { settings, allow_wasi });
}

/// Whether the operator allowed this plugin the WASI grants its manifest asks
/// for. Both halves are needed: the manifest declares, the operator allows.
pub fn wasi_allowed(plugin: &str) -> bool {
    OPERATOR.lock().as_ref().is_some_and(|o| o.allow_wasi.iter().any(|n| n == plugin))
}

/// What the operator asked for in `[plugins.<name>] place`.
fn place_of(plugin: &str) -> Option<String> {
    let guard = OPERATOR.lock();
    let table = guard.as_ref()?.settings.get(plugin)?;
    Some(table.get("place")?.as_str()?.to_string())
}

/// Whether this plugin runs as a component.
///
/// Three things have to line up: the manifest declares the `wasm` placement,
/// `[run] wasm` names a component file, and either `wasm` is the only
/// placement the plugin has or the operator wrote `place = "wasm"` in
/// `[plugins.<name>]`. A plugin that can run both ways therefore stays a
/// process until somebody asks for the other thing, which is the conservative
/// default: a process is the placement everything else in the core is built
/// around.
pub fn runs_as_wasm(manifest: &PluginManifest) -> bool {
    if !manifest.plugin.placements.iter().any(|p| p == "wasm") {
        return false;
    }
    if manifest.run.as_ref().and_then(|r| r.wasm.as_ref()).is_none() {
        return false;
    }
    place_of(&manifest.plugin.name).as_deref() == Some("wasm")
        || manifest.plugin.placements.len() == 1
}

/// A media kind was asked to run where no media goes.
///
/// Its own type so the control layer can answer -32005 with the placements
/// that do carry media, rather than matching on a string.
#[derive(Debug, Clone)]
pub struct MediaRefused {
    pub type_id: String,
    pub kind: String,
    /// The placements that carry media, for `data.placements`.
    pub placements: Vec<String>,
}

impl std::fmt::Display for MediaRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "`{}` is a {} and the `wasm` placement carries no media, so a frame would never \
             reach it. Media crosses at these placements: {}. Tier W runs {} logic only; \
             docs/explanation/why-wasm-is-not-on-the-frame-path.md says why.",
            self.type_id,
            self.kind,
            self.placements.join(", "),
            WASM_KINDS.join(", ")
        )
    }
}

impl std::error::Error for MediaRefused {}

impl MediaRefused {
    pub fn new(type_id: &str, kind: &str) -> Self {
        Self {
            type_id: type_id.to_string(),
            kind: kind.to_string(),
            placements: vec!["in-process".into(), "sidecar".into(), "node".into()],
        }
    }
}

/// Refuse a media provide at the `wasm` placement, before anything is built.
///
/// Called from the launch path, so `source.add`, `output.add` and `filter.add`
/// are all covered by one check rather than three.
pub fn check_media(type_id: &str, kind: &str, manifest: &PluginManifest) -> Result<()> {
    if WASM_KINDS.contains(&kind) {
        return Ok(());
    }
    if runs_as_wasm(manifest) {
        return Err(anyhow::Error::new(MediaRefused::new(type_id, kind)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(placements: &[&str], wasm: Option<&str>) -> PluginManifest {
        let list = placements.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(", ");
        let run = match wasm {
            Some(w) => format!("[run]\nwasm = \"{w}\"\n"),
            None => String::new(),
        };
        PluginManifest::parse(&format!(
            "[plugin]\nname = \"demo\"\nversion = \"0.1.0\"\napi = 1\n\
             placements = [{list}]\n{run}"
        ))
        .expect("the test manifest parses")
    }

    #[test]
    fn a_plugin_that_can_only_be_a_component_is_one_without_being_asked() {
        let m = manifest(&["wasm"], Some("plugin.wasm"));
        assert!(runs_as_wasm(&m));
    }

    #[test]
    fn a_plugin_that_can_be_either_stays_a_process_until_the_operator_says_otherwise() {
        let m = manifest(&["sidecar", "wasm"], Some("plugin.wasm"));
        set_config(BTreeMap::new(), Vec::new());
        assert!(!runs_as_wasm(&m), "a process is the conservative default");
        let asked: toml::Table = toml::from_str("place = \"wasm\"").expect("a table");
        set_config(BTreeMap::from([("demo".to_string(), asked)]), vec!["demo".into()]);
        assert!(runs_as_wasm(&m));
        assert!(wasi_allowed("demo") && !wasi_allowed("other"));
        set_config(BTreeMap::new(), Vec::new());
    }

    #[test]
    fn a_wasm_placement_with_no_component_file_is_not_a_component() {
        let m = manifest(&["wasm"], None);
        assert!(!runs_as_wasm(&m), "[run] wasm has to name the file");
    }

    #[test]
    fn a_source_is_refused_at_the_wasm_placement_and_the_message_names_where_media_goes() {
        let m = manifest(&["wasm"], Some("plugin.wasm"));
        let e = check_media("demo/source", "source", &m).expect_err("media never crosses");
        let refused = e.downcast_ref::<MediaRefused>().expect("a typed refusal");
        assert_eq!(refused.placements, ["in-process", "sidecar", "node"]);
        let said = e.to_string();
        assert!(said.contains("carries no media"), "{said}");
        assert!(said.contains("sidecar"), "the message must name where media does cross: {said}");
    }

    #[test]
    fn a_service_is_not_refused() {
        let m = manifest(&["wasm"], Some("plugin.wasm"));
        assert!(check_media("demo/hold", "service", &m).is_ok());
        assert!(check_media("demo/ease", "transition", &m).is_ok());
    }

    #[test]
    fn a_build_with_no_runner_says_which_flag_brings_one() {
        if present() {
            return;
        }
        let spec = Spec {
            plugin: "demo".into(),
            provide: "hold".into(),
            instance: "demo-hold".into(),
            kind: super::super::ProvideKind::Service,
            component: PathBuf::from("plugin.wasm"),
            root: PathBuf::from("."),
            canvas: CanvasCaps::new(&Default::default()),
            params: Value::Null,
            grant: Grant::default(),
        };
        let Err(e) = start(spec) else {
            panic!("no runner is registered in the core's own tests, so this must refuse");
        };
        assert!(e.to_string().contains("--features wasm"), "{e}");
    }
}
