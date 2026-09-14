//! The plugin loader: what is installed, what is enabled, and what it costs.
//!
//! Plugins live at `<plugins_dir>/<name>/<version>/`, which defaults to
//! `~/.godwinmix/plugins`. Each directory has a `gmx-plugin.toml` at its root
//! and nothing else is assumed. The loader reads them at startup and on
//! `plugin.add`, registers every provide they declare, and unwinds every
//! registration when one leaves.
//!
//! Registration is reversible on purpose. Everything a plugin contributes (a
//! provide id, a tool, a panel, a hook, a discovery matcher) is held in one
//! record, and `plugin.remove` drops the record: there is no second place a
//! half removed plugin can still be reachable from. That is Cordis's rule and
//! it is the difference between a hot unload that works and one that leaves a
//! dangling menu entry.
//!
//! The registry is a `RwLock` read on every `source.add` and written on the
//! rare occasions a plugin arrives or leaves, so a take never waits on a
//! plugin being installed.

use super::source::Provide;
use super::{
    Capability, CapabilitySet, KindInfo, Manifest, MediaDecl, ProvideKind, StreamMode, Tier,
};
use anyhow::{Context, Result};
use godwinmix_host::budget::{Budget, Stats, Watch};
use godwinmix_host::launch::{self, Launch, LaunchCtx};
use godwinmix_host::sampler::Sampler;
use godwinmix_protocol::plugin::manifest::{
    Manifest as PluginManifest, Provide as ProvideDecl, Tool,
};
use godwinmix_protocol::plugin::wire::Transport;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing::{info, warn};

/// Where plugins live when nothing says otherwise.
pub fn default_dir() -> PathBuf {
    if let Ok(explicit) = std::env::var("GODWINMIX_PLUGINS_DIR") {
        return PathBuf::from(explicit);
    }
    home().join(".godwinmix").join("plugins")
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// One installed plugin, with everything it registered.
#[derive(Debug, Clone)]
pub struct Installed {
    pub manifest: PluginManifest,
    /// `<plugins_dir>/<name>/<version>`.
    pub root: PathBuf,
    /// Off means it is installed and does nothing: no process, no provide, no
    /// tool. `plugin.enable` puts it back without reinstalling.
    pub enabled: bool,
    /// The provide ids it contributed, in manifest order. Everything in here
    /// is unwound when the plugin leaves.
    pub provides: Vec<String>,
    /// The MCP tool names it contributed, as `gmx_<plugin>_<tool>`.
    pub tools: Vec<String>,
    /// The hooks it asked to be called on.
    pub hooks: Vec<String>,
    /// `[plugins.<name>]` from the operator's config.
    pub budget: Budget,
    /// Why it is not loaded, when it is not.
    pub problem: Option<String>,
}

impl Installed {
    pub fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    pub fn version(&self) -> &str {
        &self.manifest.plugin.version
    }

    /// Is this plugin contributing anything right now?
    pub fn live(&self) -> bool {
        self.enabled && self.problem.is_none()
    }
}

/// The per instance numbers `plugin.list` and `plugin.stats` carry.
#[derive(Debug, Clone, Default)]
pub struct InstanceStats {
    pub plugin: String,
    pub provide: String,
    pub instance: String,
    pub state: String,
    pub pid: Option<u32>,
    pub stats: Stats,
}

/// Everything the loader holds.
#[derive(Default)]
struct Registry {
    dir: PathBuf,
    plugins: BTreeMap<String, Installed>,
    /// Interned provides, so a `&'static Provide` can be handed to the same
    /// lookups the built in kinds use. Interned rather than leaked per call:
    /// reloading the same plugin reuses its entry, so a core that reloads a
    /// plugin a thousand times does not grow a thousand manifests.
    interned: BTreeMap<String, &'static Provide>,
    /// Per instance numbers, refreshed once a second by the sampler.
    stats: BTreeMap<String, InstanceStats>,
    /// The pid behind each instance, so the sampler can read a set at a time.
    pids: BTreeMap<String, u32>,
    watches: BTreeMap<String, Watch>,
}

fn registry() -> &'static RwLock<Registry> {
    static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        RwLock::new(Registry {
            dir: default_dir(),
            ..Default::default()
        })
    })
}

/// Where plugins are read from. Set once at startup from `[server] plugins_dir`.
pub fn set_dir(dir: PathBuf) {
    registry().write().dir = dir;
}

pub fn dir() -> PathBuf {
    registry().read().dir.clone()
}

/// Read every plugin under the plugins directory.
///
/// A plugin whose manifest does not validate is recorded with its problem
/// rather than dropped silently: an operator who put a plugin there wants to
/// know why it is not working, and `plugin.list` is where they will look.
pub fn scan(budgets: &BTreeMap<String, crate::config::Params>) -> Vec<Installed> {
    let root = dir();
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        // No directory is the ordinary case on a fresh install, not an error.
        return found;
    };
    for entry in entries.flatten() {
        let name_dir = entry.path();
        if !name_dir.is_dir() {
            continue;
        }
        let Ok(versions) = std::fs::read_dir(&name_dir) else {
            continue;
        };
        for version in versions.flatten() {
            let path = version.path();
            if path.is_dir() && path.join("gmx-plugin.toml").is_file() {
                found.push(read(&path, budgets));
            }
        }
    }
    found
}

/// Read one plugin directory, valid or not.
pub fn read(root: &Path, budgets: &BTreeMap<String, crate::config::Params>) -> Installed {
    let manifest_path = root.join("gmx-plugin.toml");
    match PluginManifest::load(&manifest_path) {
        Ok(manifest) => {
            let budget = budgets
                .get(&manifest.plugin.name)
                .map(|t| Budget::from_table(&TomlTable(t)))
                .unwrap_or_default();
            let provides = manifest
                .provides
                .iter()
                .map(|p| format!("{}/{}", manifest.plugin.name, p.id))
                .collect();
            let tools = manifest
                .tools
                .iter()
                .map(|t| tool_name(&manifest.plugin.name, &t.name))
                .collect();
            let hooks = manifest.hooks.keys().cloned().collect();
            Installed {
                manifest,
                root: root.to_path_buf(),
                enabled: true,
                provides,
                tools,
                hooks,
                budget,
                problem: None,
            }
        }
        Err(e) => Installed {
            manifest: placeholder(root),
            root: root.to_path_buf(),
            enabled: false,
            provides: Vec::new(),
            tools: Vec::new(),
            hooks: Vec::new(),
            budget: Budget::default(),
            problem: Some(format!("{e}")),
        },
    }
}

/// The MCP name a plugin's tool is exposed under: `gmx_<plugin>_<tool>`.
///
/// Behind `search_tools`, never in the hot list. Adding a plugin therefore
/// changes nothing about the tools an agent is shown, which is what keeps the
/// prompt cache valid.
pub fn tool_name(plugin: &str, tool: &str) -> String {
    format!("gmx_{}_{}", plugin.replace('-', "_"), tool)
}

/// A manifest for a directory whose own manifest would not parse, so that
/// `plugin.list` can still name it and say why.
fn placeholder(root: &Path) -> PluginManifest {
    let name = root
        .parent()
        .and_then(Path::file_name)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".into());
    let version = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "0.0.0".into());
    let text = format!(
        "[plugin]\nname = \"{}\"\nversion = \"{}\"\napi = 1\n",
        if godwinmix_protocol::plugin::manifest::is_slug(&name) {
            name
        } else {
            "unknown".into()
        },
        version
    );
    PluginManifest::parse(&text).expect("a two field manifest parses")
}

/// A `toml::Table` read through the host's three key accessor.
struct TomlTable<'a>(&'a crate::config::Params);

impl godwinmix_host::budget::toml_like::Table for TomlTable<'_> {
    fn integer(&self, key: &str) -> Option<i64> {
        self.0.get(key).and_then(toml::Value::as_integer)
    }
    fn float(&self, key: &str) -> Option<f64> {
        self.0
            .get(key)
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
    }
    fn string(&self, key: &str) -> Option<String> {
        self.0
            .get(key)
            .and_then(toml::Value::as_str)
            .map(str::to_string)
    }
}

/// Load everything under the plugins directory into the registry.
///
/// Returns what it found, so the caller logs one line per plugin rather than
/// the loader deciding what a startup report looks like.
pub fn load_all(budgets: &BTreeMap<String, crate::config::Params>) -> Vec<Installed> {
    let found = scan(budgets);
    let mut reg = registry().write();
    reg.plugins.clear();
    for plugin in &found {
        if let Some(problem) = &plugin.problem {
            warn!(plugin = plugin.name(), "{problem}");
        } else {
            info!(
                plugin = plugin.name(),
                version = plugin.version(),
                provides = plugin.provides.len(),
                "loaded a plugin"
            );
        }
        reg.plugins
            .insert(plugin.name().to_string(), plugin.clone());
    }
    drop(reg);
    intern_all();
    found
}

/// Put one plugin in, replacing whatever was there under that name.
pub fn insert(plugin: Installed) {
    registry()
        .write()
        .plugins
        .insert(plugin.name().to_string(), plugin);
    intern_all();
}

/// Take a plugin out, and with it every provide, tool, hook and stat it
/// contributed. Answers what was removed, so the caller can say so.
pub fn remove(name: &str) -> Option<Installed> {
    let mut reg = registry().write();
    let gone = reg.plugins.remove(name)?;
    for id in &gone.provides {
        reg.interned.remove(id);
    }
    let prefix = format!("{name}/");
    reg.stats
        .retain(|k, v| !k.starts_with(&prefix) && v.plugin != name);
    reg.pids.retain(|k, _| !k.starts_with(&prefix));
    reg.watches.retain(|k, _| !k.starts_with(&prefix));
    Some(gone)
}

/// Turn a plugin on or off without reinstalling it. `None` if there is no such
/// plugin.
pub fn set_enabled(name: &str, on: bool) -> Option<Installed> {
    {
        let mut reg = registry().write();
        let plugin = reg.plugins.get_mut(name)?;
        plugin.enabled = on;
    }
    intern_all();
    registry().read().plugins.get(name).cloned()
}

/// Every plugin that is installed, enabled or not.
pub fn list() -> Vec<Installed> {
    registry().read().plugins.values().cloned().collect()
}

/// One plugin by name.
pub fn get(name: &str) -> Option<Installed> {
    registry().read().plugins.get(name).cloned()
}

/// Every plugin that is live, for a listing that only wants the working ones.
pub fn enabled() -> Vec<Installed> {
    registry()
        .read()
        .plugins
        .values()
        .filter(|p| p.live())
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Registration: how a loaded provide reaches the same lookups a built in uses
// ---------------------------------------------------------------------------

/// Intern every live provide, so `source::by_type` can answer with a
/// `&'static Provide` exactly as it does for a built in kind.
fn intern_all() {
    let plugins: Vec<Installed> = registry()
        .read()
        .plugins
        .values()
        .filter(|p| p.live())
        .cloned()
        .collect();
    let mut made: BTreeMap<String, &'static Provide> = BTreeMap::new();
    {
        let reg = registry().read();
        for plugin in &plugins {
            for decl in &plugin.manifest.provides {
                let id = format!("{}/{}", plugin.name(), decl.id);
                if let Some(existing) = reg.interned.get(&id) {
                    // Same id, same manifest: reuse rather than leak a second.
                    if existing.manifest.rank == decl.rank.unwrap_or(128) as u16 {
                        made.insert(id, *existing);
                        continue;
                    }
                }
                if decl.kind != "source" {
                    // Only sources reach the URI resolver. Outputs and filters
                    // are looked up by `type` through their own registries.
                    continue;
                }
                made.insert(id.clone(), intern_provide(plugin, decl));
            }
        }
    }
    registry().write().interned = made;
}

/// Make one `&'static Provide` for a plugin's source provide.
///
/// The strings are leaked once per distinct provide. A core that installs
/// twenty plugins leaks twenty manifests' worth of short strings and never
/// grows again, which is the price of letting a runtime provide sit in the
/// same table as a compiled in one and be looked up with no allocation on the
/// hot path.
fn intern_provide(plugin: &Installed, decl: &ProvideDecl) -> &'static Provide {
    let manifest = Manifest {
        plugin: leak(plugin.name()),
        id: leak(&decl.id),
        kind: ProvideKind::Source,
        api: plugin.manifest.plugin.api,
        description: leak(&plugin.manifest.plugin.description),
        uri_schemes: leak_list(&decl.uri_schemes),
        rank: decl.rank.unwrap_or(128).min(256) as u16,
        media: media_of(decl),
        capabilities: capabilities_of(decl),
        latency_ms: decl.latency_ms.unwrap_or(0),
        tier: Tier::Sidecar,
    };
    Box::leak(Box::new(Provide {
        manifest,
        claims: claims_by_scheme,
        make: super::host::make_source,
    }))
}

/// A sidecar source claims a bare URI by the schemes it declared, at the rank
/// it declared, exactly as a built in kind does.
fn claims_by_scheme(uri: &str) -> Option<u16> {
    let lower = uri.trim().to_lowercase();
    registry()
        .read()
        .interned
        .values()
        .filter(|p| p.manifest.uri_schemes.iter().any(|s| lower.starts_with(*s)))
        .map(|p| p.manifest.rank)
        .max()
}

fn media_of(decl: &ProvideDecl) -> MediaDecl {
    let mode = |s: &str| match s {
        "raw" => StreamMode::Raw,
        "container" => StreamMode::Container,
        _ => StreamMode::None,
    };
    match &decl.media {
        Some(m) => MediaDecl {
            video: mode(&m.video),
            audio: mode(&m.audio),
            alpha: m.alpha,
            thumb: m.thumb,
        },
        None => MediaDecl {
            video: StreamMode::None,
            audio: StreamMode::None,
            alpha: false,
            thumb: false,
        },
    }
}

fn capabilities_of(decl: &ProvideDecl) -> CapabilitySet {
    let mut caps = CapabilitySet::new();
    for name in &decl.capabilities {
        if let Some(c) = Capability::parse(name) {
            caps.set(c, true);
        }
    }
    caps
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn leak_list(items: &[String]) -> &'static [&'static str] {
    let leaked: Vec<&'static str> = items.iter().map(|s| leak(s)).collect();
    Box::leak(leaked.into_boxed_slice())
}

/// The provide a `type` names, if a loaded plugin has one.
///
/// Consulted by `source::by_type` after the built in registry, so a plugin can
/// never shadow a kind that ships with the core.
pub fn source_provide(type_id: &str) -> Option<&'static Provide> {
    let reg = registry().read();
    if let Some(p) = reg.interned.get(type_id) {
        return Some(*p);
    }
    // A bare plugin name means its default provide of this kind.
    reg.interned
        .iter()
        .find(|(id, _)| id.split('/').next() == Some(type_id))
        .map(|(_, p)| *p)
}

/// The loaded provide a bare URI resolves to, by scheme and rank.
pub fn source_for_uri(uri: &str) -> Option<&'static Provide> {
    let lower = uri.trim().to_lowercase();
    registry()
        .read()
        .interned
        .values()
        .filter(|p| p.manifest.uri_schemes.iter().any(|s| lower.starts_with(*s)))
        .max_by_key(|p| p.manifest.rank)
        .copied()
}

/// Every provide id a loaded plugin contributes, for an error that lists what
/// this core can actually make.
pub fn available() -> Vec<String> {
    registry().read().interned.keys().cloned().collect()
}

/// Every loaded kind, for `core.api`'s `kinds` table and the add gallery.
pub fn described() -> Vec<KindInfo> {
    registry()
        .read()
        .interned
        .values()
        .map(|p| p.manifest.describe())
        .collect()
}

/// The settings schema a provide declared, read off disk.
///
/// Part of `core.api`'s `kinds` entry for a plugin provide, so a surface
/// renders a plugin's settings with the same code it renders a built in kind's.
pub fn settings_schema(type_id: &str) -> Option<serde_json::Value> {
    let (name, id) = type_id.split_once('/')?;
    let plugin = get(name)?;
    let decl = plugin.manifest.provide(id)?;
    let path = plugin.root.join(decl.settings.as_ref()?);
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

// ---------------------------------------------------------------------------
// What the engine cannot know on its own
// ---------------------------------------------------------------------------

/// Where the core keeps its runtime files. Sockets and FIFOs for an instance
/// go under `<runtime>/plugins/<instance>/`.
pub fn runtime_dir() -> PathBuf {
    RUNTIME.get().cloned().unwrap_or_else(std::env::temp_dir)
}

pub fn set_runtime_dir(dir: PathBuf) {
    let _ = RUNTIME.set(dir);
}

static RUNTIME: OnceLock<PathBuf> = OnceLock::new();

/// The WebSocket URL of the core's own `/rpc`, handed to every plugin in
/// `GMX_RPC`. Empty until the server has bound a port, which is the honest
/// answer for an embedded core with no server at all.
pub fn rpc_url() -> String {
    RPC.get().cloned().unwrap_or_default()
}

pub fn set_rpc_url(url: String) {
    let _ = RPC.set(url);
}

static RPC: OnceLock<String> = OnceLock::new();

/// How a per instance token is made.
///
/// The engine has no idea what a token is: scopes, confirmation and the
/// rehearsal flag all live in the protocol crate and are minted by the server.
/// So the server installs a minter at startup and the loader asks it. Without
/// one, a plugin is given an empty `GMX_TOKEN`, which is what an embedded core
/// with no control server should hand out.
type Minter = Box<dyn Fn(&str, &str) -> String + Send + Sync>;

static MINTER: OnceLock<Minter> = OnceLock::new();

pub fn set_token_minter(minter: Minter) {
    let _ = MINTER.set(minter);
}

/// A token for one instance, scoped `plugin:<name>`.
pub fn mint_token(type_id: &str, instance: &str) -> String {
    let plugin = type_id.split('/').next().unwrap_or(type_id);
    match MINTER.get() {
        Some(mint) => mint(plugin, instance),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Launching
// ---------------------------------------------------------------------------

/// Everything needed to start one instance of a provide.
#[derive(Debug)]
pub struct Launched {
    pub plugin: PluginManifest,
    pub provide: String,
    pub launch: Launch,
    pub ctx: LaunchCtx,
    pub transports: Vec<Transport>,
}

/// Build the launch plan for one instance of `type_id`.
///
/// The token is the per instance one, scoped `plugin:<name>`, issued by the
/// caller because only the server knows how to mint one.
pub fn launch_for(type_id: &str, instance: &str, token: String, rpc: String) -> Result<Launched> {
    let (name, id) = type_id.split_once('/').with_context(|| {
        format!("`{type_id}` is not a plugin provide id; write <plugin>/<provide>")
    })?;
    let plugin = get(name).with_context(|| {
        let have = list()
            .iter()
            .map(|p| p.name().to_string())
            .collect::<Vec<_>>();
        format!(
            "no plugin called `{name}` is installed. Installed: {}. Add one with \
             `gmx plugin add <path>`.",
            if have.is_empty() {
                "none".into()
            } else {
                have.join(", ")
            }
        )
    })?;
    anyhow::ensure!(
        plugin.enabled,
        "the plugin `{name}` is installed but disabled. Turn it on with \
         `gmx plugin enable {name}`."
    );
    if let Some(problem) = &plugin.problem {
        anyhow::bail!("the plugin `{name}` did not load: {problem}");
    }
    let decl = plugin.manifest.provide(id).with_context(|| {
        let have: Vec<&str> = plugin
            .manifest
            .provides
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        format!(
            "`{name}` has no provide called `{id}`. It provides: {}.",
            have.join(", ")
        )
    })?;
    anyhow::ensure!(
        plugin
            .manifest
            .plugin
            .placements
            .iter()
            .any(|p| p == "sidecar"),
        "the plugin `{name}` does not declare the `sidecar` placement. It declares: {}.",
        plugin.manifest.plugin.placements.join(", ")
    );
    let ctx = LaunchCtx {
        root: plugin.root.clone(),
        provide: id.to_string(),
        instance: instance.to_string(),
        api_level: super::API_LEVEL,
        token,
        rpc,
        media: String::new(),
    };
    let launch = launch::plan(&plugin.manifest, &ctx)?;
    Ok(Launched {
        plugin: plugin.manifest.clone(),
        provide: id.to_string(),
        launch,
        ctx,
        transports: decl.transports.clone(),
    })
}

// ---------------------------------------------------------------------------
// Installing and uninstalling
// ---------------------------------------------------------------------------

/// Copy a plugin from a local directory into `<plugins_dir>/<name>/<version>`.
///
/// Phase 2 takes a path and nothing else. A git source, an index and a
/// signature are Phase 5, and `plugin.add` says so by name rather than failing
/// on a URL with a parse error.
pub fn install_from_path(source: &Path) -> Result<Installed> {
    anyhow::ensure!(
        source.is_dir(),
        "`{}` is not a directory. `plugin.add` takes the directory a plugin was built in, the \
         one with gmx-plugin.toml at its root.",
        source.display()
    );
    let manifest_path = source.join("gmx-plugin.toml");
    anyhow::ensure!(
        manifest_path.is_file(),
        "there is no gmx-plugin.toml in `{}`. Every plugin has one at its root; \
         `gmx plugin new` writes it.",
        source.display()
    );
    let manifest = PluginManifest::load(&manifest_path).map_err(|e| anyhow::anyhow!("{e}"))?;
    let name = manifest.plugin.name.clone();
    let version = manifest.plugin.version.clone();
    anyhow::ensure!(
        manifest
            .plugin
            .platforms
            .iter()
            .any(|p| p == launch::this_platform()),
        "`{name}` has no asset for {}. It ships: {}. Install it from a git source with a \
         [build] section, or ask the author for this platform.",
        launch::this_platform(),
        manifest.plugin.platforms.join(", ")
    );
    let target = dir().join(&name).join(&version);
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .with_context(|| format!("replacing {}", target.display()))?;
    }
    copy_tree(source, &target)?;
    let installed = read(&target, &BTreeMap::new());
    if let Some(problem) = &installed.problem {
        // Do not leave a broken copy behind for the next scan to find.
        let _ = std::fs::remove_dir_all(&target);
        anyhow::bail!("{problem}");
    }
    insert(installed.clone());
    info!(plugin = %name, version = %version, at = %target.display(), "installed a plugin");
    Ok(installed)
}

/// Take a plugin's directory off the disk as well as out of the registry.
///
/// Every version of it: `plugin.remove ndi` means the plugin is gone, not that
/// one of two installed versions is.
pub fn uninstall(name: &str) -> Result<Installed> {
    let gone = remove(name).with_context(|| {
        let have: Vec<String> = list().iter().map(|p| p.name().to_string()).collect();
        format!(
            "no plugin called `{name}` is installed. Installed: {}.",
            if have.is_empty() {
                "none".into()
            } else {
                have.join(", ")
            }
        )
    })?;
    let root = dir().join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).with_context(|| format!("removing {}", root.display()))?;
    }
    info!(plugin = %name, "removed a plugin");
    Ok(gone)
}

/// Copy a directory tree. No symlinks are followed: a plugin that installs a
/// link out of its own root is a plugin that escapes it.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("making {}", to.display()))?;
    for entry in std::fs::read_dir(from)
        .with_context(|| format!("reading {}", from.display()))?
        .flatten()
    {
        let path = entry.path();
        let name = entry.file_name();
        // A build tree, a virtual environment and a repository are not part of
        // a plugin and are often larger than one.
        if matches!(
            name.to_string_lossy().as_ref(),
            ".git" | "target" | "node_modules" | ".venv" | "__pycache__"
        ) {
            continue;
        }
        let kind = entry
            .file_type()
            .with_context(|| format!("reading {}", path.display()))?;
        if kind.is_symlink() {
            warn!(at = %path.display(), "skipping a symlink while installing a plugin");
            continue;
        }
        if kind.is_dir() {
            copy_tree(&path, &to.join(name))?;
        } else {
            let target = to.join(name);
            std::fs::copy(&path, &target).with_context(|| format!("copying {}", path.display()))?;
            copy_mode(&path, &target);
        }
    }
    Ok(())
}

/// Keep the executable bit. A plugin whose binary arrives without it starts
/// with "permission denied" and nothing saying why.
#[cfg(unix)]
fn copy_mode(from: &Path, to: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(from) {
        let mode = meta.permissions().mode();
        let _ = std::fs::set_permissions(to, std::fs::Permissions::from_mode(mode));
    }
}

#[cfg(not(unix))]
fn copy_mode(_from: &Path, _to: &Path) {}

// ---------------------------------------------------------------------------
// Stats and budgets
// ---------------------------------------------------------------------------

/// Tell the loader which process is behind an instance, so the sampler can
/// read it. Called when an instance starts, and with `None` when it stops.
pub fn set_pid(instance: &str, plugin: &str, provide: &str, pid: Option<u32>) {
    let mut reg = registry().write();
    match pid {
        Some(pid) => {
            reg.pids.insert(instance.to_string(), pid);
            let entry = reg.stats.entry(instance.to_string()).or_default();
            entry.plugin = plugin.to_string();
            entry.provide = provide.to_string();
            entry.instance = instance.to_string();
            entry.pid = Some(pid);
        }
        None => {
            reg.pids.remove(instance);
            if let Some(entry) = reg.stats.get_mut(instance) {
                entry.pid = None;
            }
        }
    }
}

/// Record what an instance is doing, for `plugin.list`.
pub fn set_state(instance: &str, state: &str) {
    if let Some(entry) = registry().write().stats.get_mut(instance) {
        entry.state = state.to_string();
    }
}

/// Count a restart against an instance.
pub fn count_restart(instance: &str) {
    if let Some(entry) = registry().write().stats.get_mut(instance) {
        entry.stats.restarts += 1;
    }
}

/// What one breach means for one instance.
#[derive(Debug, Clone)]
pub struct Breach {
    pub instance: String,
    pub plugin: String,
    pub reason: String,
    pub action: godwinmix_host::budget::OverBudget,
}

/// Refresh every instance's numbers, and say which ones broke their budget.
///
/// Called once a second by the supervisor tick. One `ps` (or one read of
/// `/proc`) for every instance at a time, which is the whole cost.
pub fn refresh_stats(sampler: &mut Sampler) -> Vec<Breach> {
    let pids: Vec<u32> = registry().read().pids.values().copied().collect();
    if pids.is_empty() {
        return Vec::new();
    }
    let samples = sampler.sample(&pids);
    let mut breaches = Vec::new();
    let mut reg = registry().write();
    let budgets: BTreeMap<String, Budget> = reg
        .plugins
        .iter()
        .map(|(k, v)| (k.clone(), v.budget))
        .collect();
    let instances: Vec<(String, u32)> = reg.pids.iter().map(|(k, v)| (k.clone(), *v)).collect();
    for (instance, pid) in instances {
        let Some(sample) = samples.get(&pid) else {
            continue;
        };
        let Some(entry) = reg.stats.get_mut(&instance) else {
            continue;
        };
        entry.stats.cpu_percent = sample.cpu_percent;
        entry.stats.rss_bytes = sample.rss_bytes;
        let plugin = entry.plugin.clone();
        let stats = entry.stats;
        let Some(budget) = budgets.get(&plugin).filter(|b| b.is_set()) else {
            continue;
        };
        let watch = reg.watches.entry(instance.clone()).or_default();
        if let Some(reason) = watch.observe(budget, &stats) {
            breaches.push(Breach {
                instance: instance.clone(),
                plugin,
                reason,
                action: budget.on_over_budget,
            });
        }
    }
    breaches
}

/// Start the once a second refresh of every instance's numbers.
///
/// One thread for the whole core, not one per plugin, and it does nothing at
/// all while no plugin instance is running: `refresh_stats` returns before it
/// reads anything when there are no pids. Nothing runs unless asked.
///
/// The breaches it finds are handed to `on_breach`, because what to do about
/// one (restart the instance, disable it, raise an alert) needs a mixer and the
/// loader has none.
pub fn start_sampler(on_breach: impl Fn(Breach) + Send + 'static) {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("plugin-stats".into())
        .spawn(move || {
            let mut sampler = Sampler::new();
            loop {
                for breach in refresh_stats(&mut sampler) {
                    tracing::warn!(
                        plugin = %breach.plugin,
                        instance = %breach.instance,
                        action = breach.action.as_str(),
                        "{} is over its budget: {}",
                        breach.instance,
                        breach.reason
                    );
                    on_breach(breach);
                }
                std::thread::sleep(godwinmix_host::sampler::REFRESH);
            }
        })
        .ok();
}

/// Every instance's numbers. What `plugin.stats` answers with.
pub fn stats() -> Vec<InstanceStats> {
    registry().read().stats.values().cloned().collect()
}

/// One instance's numbers.
pub fn stats_for(instance: &str) -> Option<InstanceStats> {
    registry().read().stats.get(instance).cloned()
}

/// Record the latency a plugin reported, so `plugin.list` can show it.
pub fn set_latency(instance: &str, ms: u32) {
    if let Some(entry) = registry().write().stats.get_mut(instance) {
        entry.stats.media_latency_ms = Some(ms);
    }
}

/// Every `[[tools]]` a live plugin contributes, with the plugin it came from.
///
/// Reached through `search_tools`, never through the hot list: adding a plugin
/// must not change the tools an agent is shown, or the prompt cache is thrown
/// away on every install.
pub fn tools() -> Vec<(String, String, Tool)> {
    enabled()
        .into_iter()
        .flat_map(|p| {
            let name = p.name().to_string();
            p.manifest
                .tools
                .clone()
                .into_iter()
                .map(move |t| (tool_name(&name, &t.name), name.clone(), t))
        })
        .collect()
}

/// Forget everything. Tests only: the registry is a process wide singleton
/// and a test that installs a plugin must not leave it there for the next one.
#[cfg(test)]
pub fn clear() {
    let mut reg = registry().write();
    reg.plugins.clear();
    reg.interned.clear();
    reg.stats.clear();
    reg.pids.clear();
    reg.watches.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry is one per process, because a plugin table that differed
    /// between two parts of the same core would be a bug. That makes every
    /// test here a test of shared state, so they take a lock and hand it back
    /// rather than racing each other into each other's assertions.
    fn exclusive() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        guard
    }

    /// A plugin directory with a manifest that validates, and nothing else.
    fn write_plugin(root: &Path, name: &str, version: &str, extra: &str) -> PathBuf {
        let dir = root.join(name).join(version);
        std::fs::create_dir_all(&dir).expect("the plugin directory is made");
        std::fs::write(dir.join("settings.json"), "{\"type\":\"object\"}").expect("settings");
        std::fs::write(
            dir.join("gmx-plugin.toml"),
            format!(
                r#"
[plugin]
name = "{name}"
version = "{version}"
api = 1
description = "A plugin the loader tests read."
license = "MIT"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64"]
placements = ["sidecar"]

[run]
shell = "run.sh"

[[provides]]
kind = "source"
id = "source"
uri_schemes = ["{name}://"]
rank = 200
media = {{ video = "raw", audio = "none", thumb = true }}
transports = ["container"]
capabilities = ["restart-in-place", "health"]
settings = "settings.json"
{extra}
"#
            ),
        )
        .expect("the manifest is written");
        std::fs::write(dir.join("run.sh"), "#!/bin/sh\nexit 0\n").expect("the entry point");
        dir
    }

    fn temp(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "gmx-loader-{}-{}-{tag}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a temporary plugins directory");
        path
    }

    #[test]
    fn a_plugin_directory_is_read_with_its_version() {
        let root = temp("read");
        let dir = write_plugin(&root, "clock", "0.1.0", "");
        let installed = read(&dir, &BTreeMap::new());
        assert_eq!(installed.problem, None, "{:?}", installed.problem);
        assert_eq!(installed.name(), "clock");
        assert_eq!(installed.version(), "0.1.0");
        assert_eq!(installed.provides, vec!["clock/source"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_manifest_that_does_not_validate_is_listed_with_its_reason() {
        let root = temp("broken");
        let dir = root.join("broken").join("0.1.0");
        std::fs::create_dir_all(&dir).expect("the directory");
        std::fs::write(dir.join("gmx-plugin.toml"), "[plugin]\nname = \"Broken\"\n")
            .expect("a manifest that will not do");
        let installed = read(&dir, &BTreeMap::new());
        let problem = installed.problem.clone().expect("it does not validate");
        assert!(problem.contains("gmx-plugin.toml"), "{problem}");
        assert!(
            !installed.live(),
            "a plugin that did not load contributes nothing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_tool_is_named_for_search_and_never_for_the_hot_list() {
        assert_eq!(tool_name("ndi", "list_senders"), "gmx_ndi_list_senders");
        // A hyphen in a plugin name is legal and an MCP tool name's is not.
        assert_eq!(tool_name("colour-bars", "draw"), "gmx_colour_bars_draw");
    }

    #[test]
    fn a_budget_comes_off_the_operators_config_by_plugin_name() {
        let root = temp("budget");
        let dir = write_plugin(&root, "heavy", "1.0.0", "");
        let mut budgets = BTreeMap::new();
        let table: crate::config::Params =
            toml::from_str("max_rss_mb = 256\nmax_cpu_percent = 40\non_over_budget = \"disable\"")
                .expect("a budget table");
        budgets.insert("heavy".to_string(), table);
        let installed = read(&dir, &budgets);
        assert_eq!(installed.budget.max_rss_mb, Some(256));
        assert_eq!(installed.budget.max_cpu_percent, Some(40.0));
        assert_eq!(
            installed.budget.on_over_budget,
            godwinmix_host::budget::OverBudget::Disable
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_a_plugin_unwinds_everything_it_registered() {
        let _lock = exclusive();
        let root = temp("unwind");
        let dir = write_plugin(&root, "gone", "0.1.0", "");
        insert(read(&dir, &BTreeMap::new()));
        assert!(
            source_provide("gone/source").is_some(),
            "it registered its provide"
        );
        assert!(available().contains(&"gone/source".to_string()));
        set_pid("cam1", "gone", "source", Some(std::process::id()));
        assert!(stats_for("cam1").is_some());
        let gone = remove("gone").expect("it was there");
        assert_eq!(gone.name(), "gone");
        assert!(
            source_provide("gone/source").is_none(),
            "the provide went with it"
        );
        assert!(available().is_empty(), "nothing is left in the table");
        assert!(
            stats().iter().all(|s| s.plugin != "gone"),
            "its numbers went too"
        );
        clear();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_disabled_plugin_registers_nothing_and_comes_back_without_reinstalling() {
        let _lock = exclusive();
        let root = temp("disable");
        let dir = write_plugin(&root, "toggle", "0.1.0", "");
        insert(read(&dir, &BTreeMap::new()));
        assert!(source_provide("toggle/source").is_some());
        set_enabled("toggle", false).expect("it is installed");
        assert!(
            source_provide("toggle/source").is_none(),
            "off means it contributes nothing"
        );
        assert!(get("toggle").is_some(), "and it is still installed");
        set_enabled("toggle", true).expect("it is installed");
        assert!(
            source_provide("toggle/source").is_some(),
            "on brings it back"
        );
        clear();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bare_uri_reaches_a_loaded_plugin_by_scheme_and_rank() {
        let _lock = exclusive();
        let root = temp("uri");
        insert(read(
            &write_plugin(&root, "ndi", "1.0.0", ""),
            &BTreeMap::new(),
        ));
        let found = source_for_uri("ndi://CAM 1 (Studio)").expect("the scheme matches");
        assert_eq!(found.manifest.provide_id(), "ndi/source");
        assert_eq!(found.manifest.tier, Tier::Sidecar);
        assert!(
            source_for_uri("rtmp://host/app").is_none(),
            "it claims only its own scheme"
        );
        clear();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unknown_plugin_names_what_is_installed_and_how_to_add_one() {
        let _lock = exclusive();
        let err = launch_for("nope/source", "cam1", "t".into(), "ws://x/rpc".into())
            .expect_err("nothing is installed");
        let text = format!("{err}");
        assert!(text.contains("gmx plugin add"), "{text}");
        clear();
    }
}
