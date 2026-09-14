//! `plugin.*`: what is installed, what it costs, and how to change it.
//!
//! Every one of these is a thin layer over `godwinmix_core::plugin::loader`.
//! The loader owns the registry; this file turns a call into a loader request
//! and turns a refusal back into an error that names the next step.
//!
//! Two things are worth knowing before reading. `plugin.list` carries the per
//! instance cost numbers (cpu, rss, latency, dropped buffers, restarts) because
//! an operator can only drop dead weight if they can see what it weighs, and
//! the numbers are read from a table the supervisor refreshes once a second
//! rather than measured inside the call. And a plugin's `[[tools]]` reach MCP
//! through `search_tools` and never through the hot list: adding a plugin must
//! not change the tools an agent is shown, or the prompt cache is thrown away
//! on every install.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::plugin::loader;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "plugin.list",
            Scope::Read,
            "Every plugin installed, with what it provides and what each running instance \
             is costing in cpu, memory, latency, dropped buffers and restarts.",
            handler(list),
        )
        .result(schema_of::<PluginListing>)
        .tool(
            "list_plugins",
            Tier::Search,
            "Every plugin on this mixer: its name, version, whether it is enabled, the \
             type ids it provides (which is what you write in `type` when adding a source \
             or an output), and per instance cpu, memory and restart counts. Use it before \
             adding a source whose type you are not sure exists here, and use it when \
             something is slow to see which plugin is costing the most.",
        ),
    );

    reg.register(
        MethodDef::new(
            "plugin.describe",
            Scope::Read,
            "One plugin in full: its manifest, the settings schema of every provide, and \
             the description from each SKILL.md.",
            handler(describe),
        )
        .params(schema_of::<PluginName>)
        .result(schema_of::<PluginDescription>)
        .tool(
            "describe_plugin",
            Tier::Search,
            "Everything about one plugin: the manifest, the JSON Schema for each thing it \
             provides (so you know what `params` to pass), and the skill description that \
             says when to use it. Call it after `list_plugins` when you need the settings \
             for a type you are about to add. Example: describe_plugin {name: \"ndi\"}.",
        ),
    );

    reg.register(
        MethodDef::new(
            "plugin.stats",
            Scope::Read,
            "Per instance cpu, memory, media latency, dropped buffers and restarts, \
             refreshed once a second.",
            handler(stats),
        )
        .result(schema_of::<StatsListing>),
    );

    reg.register(
        MethodDef::new(
            "plugin.add",
            Scope::Admin,
            "Install a plugin from a local directory, while live. The directory is the one \
             with gmx-plugin.toml at its root.",
            handler(add),
        )
        .params(schema_of::<AddPluginRequest>)
        .result(schema_of::<PluginRecord>)
        .destructive(),
    );

    reg.register(
        MethodDef::new(
            "plugin.remove",
            Scope::Admin,
            "Uninstall a plugin and unwind everything it registered: its provides, its \
             tools, its panels, its hooks and its discovery matchers.",
            handler(remove),
        )
        .params(schema_of::<PluginName>)
        .result(schema_of::<PluginRemoved>)
        .destructive(),
    );

    reg.register(
        MethodDef::new(
            "plugin.enable",
            Scope::Admin,
            "Turn a plugin back on. It registers what it declares and its instances start.",
            handler(enable),
        )
        .params(schema_of::<PluginName>)
        .result(schema_of::<PluginRecord>),
    );

    reg.register(
        MethodDef::new(
            "plugin.disable",
            Scope::Admin,
            "Turn a plugin off without uninstalling it. It registers nothing and runs no \
             process until it is enabled again.",
            handler(disable),
        )
        .params(schema_of::<PluginName>)
        .result(schema_of::<PluginRecord>),
    );

    reg.register(
        MethodDef::new(
            "plugin.reload",
            Scope::Admin,
            "Read a plugin's directory again and swap its running instances one at a time, \
             with the freeze frame covering each.",
            handler(reload),
        )
        .params(schema_of::<PluginName>)
        .result(schema_of::<PluginRecord>),
    );

    reg.register(
        MethodDef::new(
            "plugin.settings.get",
            Scope::Read,
            "A plugin's settings as they stand, with its schema beside them.",
            handler(settings_get),
        )
        .params(schema_of::<PluginName>)
        .result(schema_of::<PluginSettings>),
    );

    reg.register(
        MethodDef::new(
            "plugin.settings.set",
            Scope::Admin,
            "Change a plugin's settings. A plugin that cannot take a change while running \
             says so rather than being restarted behind your back.",
            handler(settings_set),
        )
        .params(schema_of::<SetSettingsRequest>)
        .result(schema_of::<PluginSettings>),
    );
}

/// Anything that names one plugin.
///
/// The field is `id` because that is what the REST layer fills in from
/// `/api/v1/plugins/{id}`, and a plugin's id is its name: the namespace of
/// every id it contributes. `name` is accepted as well, for a JSON-RPC caller
/// who wrote the obvious thing.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginName {
    #[serde(alias = "name")]
    pub id: String,
}

/// `plugin.add`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddPluginRequest {
    /// A local directory with `gmx-plugin.toml` at its root. Git, an index and
    /// a signed release are Phase 5; this takes a path.
    pub source: String,
}

/// `plugin.settings.set`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SetSettingsRequest {
    #[serde(alias = "name")]
    pub id: String,
    /// Only the keys named are changed.
    #[serde(default)]
    pub settings: Map<String, Value>,
}

/// One plugin as the core reports it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginRecord {
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    /// Where it is installed.
    pub root: String,
    /// The type ids it contributes: what goes in `type` on a source, an output
    /// or a filter.
    pub provides: Vec<String>,
    /// Its MCP tools, as `gmx_<plugin>_<tool>`. Reachable with `search_tools`;
    /// never in the hot list.
    pub tools: Vec<String>,
    pub hooks: Vec<String>,
    /// Why it is not loaded, when it is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// Every running instance of it, with what it costs.
    pub instances: Vec<InstanceRecord>,
}

/// One running instance and its cost.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InstanceRecord {
    pub instance: String,
    /// The plugin it belongs to. Carried on the instance as well as on the
    /// plugin, because `plugin.stats` is a flat list and a caller holding one
    /// row should not have to go back for the name.
    pub plugin: String,
    pub provide: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rss_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_latency_ms: Option<u32>,
    pub buffers_dropped: u64,
    pub restarts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginListing {
    pub plugins: Vec<PluginRecord>,
    /// Where plugins are read from on this machine.
    pub plugins_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatsListing {
    pub instances: Vec<InstanceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginRemoved {
    pub removed: String,
    /// What went with it, so a caller can see the blast radius.
    pub provides: Vec<String>,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginSettings {
    pub name: String,
    pub settings: Map<String, Value>,
    /// The JSON Schema every surface renders, one per provide.
    pub schemas: Map<String, Value>,
}

/// The whole of one plugin, for an agent about to use it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginDescription {
    #[serde(flatten)]
    pub plugin: PluginRecord,
    /// The manifest as JSON, every table of it.
    pub manifest: Value,
    /// Per provide id, its settings schema.
    pub schemas: Map<String, Value>,
    /// Per provide id, the description line from its SKILL.md.
    pub skills: Map<String, Value>,
}

fn record(installed: &loader::Installed) -> PluginRecord {
    let instances = loader::stats()
        .into_iter()
        .filter(|s| s.plugin == installed.name())
        .map(instance)
        .collect();
    PluginRecord {
        name: installed.name().to_string(),
        version: installed.version().to_string(),
        description: installed.manifest.plugin.description.clone(),
        enabled: installed.enabled,
        root: installed.root.to_string_lossy().into_owned(),
        provides: installed.provides.clone(),
        tools: installed.tools.clone(),
        hooks: installed.hooks.clone(),
        problem: installed.problem.clone(),
        instances,
    }
}

fn instance(stats: loader::InstanceStats) -> InstanceRecord {
    InstanceRecord {
        instance: stats.instance,
        plugin: stats.plugin,
        provide: stats.provide,
        state: if stats.state.is_empty() { "stopped".into() } else { stats.state },
        pid: stats.pid,
        cpu_percent: stats.stats.cpu_percent,
        rss_bytes: stats.stats.rss_bytes,
        media_latency_ms: stats.stats.media_latency_ms,
        buffers_dropped: stats.stats.buffers_dropped,
        restarts: stats.stats.restarts,
    }
}

/// The plugin with this name, or a `-32004` that lists the ones there are.
fn find(name: &str) -> Result<loader::Installed, RpcError> {
    loader::get(name).ok_or_else(|| {
        let have: Vec<String> = loader::list().iter().map(|p| p.name().to_string()).collect();
        RpcError::not_found("plugin", name, &have)
    })
}

async fn list(_call: Call, _params: Value) -> Result<Value, RpcError> {
    body(PluginListing {
        plugins: loader::list().iter().map(record).collect(),
        plugins_dir: loader::dir().to_string_lossy().into_owned(),
    })
}

async fn stats(_call: Call, _params: Value) -> Result<Value, RpcError> {
    body(StatsListing { instances: loader::stats().into_iter().map(instance).collect() })
}

async fn describe(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PluginName = call.params(&params)?;
    let installed = find(&req.id)?;
    let manifest = serde_json::to_value(&installed.manifest)
        .map_err(|e| RpcError::internal(format!("encoding the manifest: {e}")))?;
    let mut schemas = Map::new();
    let mut skills = Map::new();
    for provide in &installed.manifest.provides {
        let id = format!("{}/{}", installed.name(), provide.id);
        if let Some(schema) = loader::settings_schema(&id) {
            schemas.insert(id.clone(), schema);
        }
        if let Some(path) = &provide.skill {
            if let Ok(text) = std::fs::read_to_string(installed.root.join(path)) {
                if let Some(skill) = godwinmix_protocol::plugin::skill::parse(&text) {
                    skills.insert(
                        id,
                        serde_json::json!({
                            "name": skill.name,
                            "description": skill.description,
                        }),
                    );
                }
            }
        }
    }
    body(PluginDescription { plugin: record(&installed), manifest, schemas, skills })
}

async fn add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: AddPluginRequest = call.params(&params)?;
    let path = std::path::PathBuf::from(&req.source);
    if !path.exists() {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!(
                "there is nothing at `{}`. `plugin.add` takes a local directory with \
                 gmx-plugin.toml at its root; installing from a git URL or an index is not \
                 in this release.",
                req.source
            ),
        )
        .with("source", req.source));
    }
    if call.dry_run {
        let manifest = godwinmix_protocol::plugin::manifest::Manifest::load(
            path.join("gmx-plugin.toml"),
        )
        .map_err(|e| RpcError::invalid_params(format!("{e}")))?;
        return Ok(call.dry_run_answer(
            true,
            vec![format!(
                "install {} v{} from {}, adding {} provide(s)",
                manifest.plugin.name,
                manifest.plugin.version,
                path.display(),
                manifest.provides.len()
            )],
        ));
    }
    // Fast enough to finish inside the call: a local copy of a directory and a
    // manifest parse. A git source has to build, which is what the task id in
    // 03 section 6 is for, and that arrives with git in Phase 5.
    let installed = loader::install_from_path(&path)
        .map_err(|e| RpcError::invalid_params(format!("{e:#}")))?;
    body(record(&installed))
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PluginName = call.params(&params)?;
    let installed = find(&req.id)?;
    if call.dry_run {
        return Ok(call.dry_run_answer(
            true,
            vec![format!(
                "remove {} v{}, unregistering {} and {} tool(s)",
                installed.name(),
                installed.version(),
                installed.provides.join(", "),
                installed.tools.len()
            )],
        ));
    }
    let gone = loader::uninstall(&req.id)
        .map_err(|e| RpcError::new(ErrorCode::NotInState, format!("{e:#}")))?;
    body(PluginRemoved {
        removed: gone.name().to_string(),
        provides: gone.provides,
        tools: gone.tools,
    })
}

async fn enable(call: Call, params: Value) -> Result<Value, RpcError> {
    set_enabled(call, params, true).await
}

async fn disable(call: Call, params: Value) -> Result<Value, RpcError> {
    set_enabled(call, params, false).await
}

async fn set_enabled(call: Call, params: Value, on: bool) -> Result<Value, RpcError> {
    let req: PluginName = call.params(&params)?;
    find(&req.id)?;
    let installed = loader::set_enabled(&req.id, on)
        .ok_or_else(|| RpcError::not_found("plugin", &req.id, &[]))?;
    body(record(&installed))
}

async fn reload(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PluginName = call.params(&params)?;
    let installed = find(&req.id)?;
    // Read the directory again, in place. Every instance keeps running until
    // its own turn comes, and the freeze frame covers each swap, which is what
    // the supervisor already does for a source being rebuilt.
    let fresh = loader::read(&installed.root, &call.app.plugin_settings);
    if let Some(problem) = &fresh.problem {
        return Err(RpcError::new(
            ErrorCode::NotInState,
            format!(
                "{} was not reloaded and the running one is untouched: {problem}",
                req.id
            ),
        ));
    }
    loader::insert(fresh.clone());
    body(record(&fresh))
}

async fn settings_get(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PluginName = call.params(&params)?;
    let installed = find(&req.id)?;
    body(settings_of(&call, &installed))
}

async fn settings_set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SetSettingsRequest = call.params(&params)?;
    let installed = find(&req.id)?;
    // The settings live in the operator's config, which is the one place a
    // restart reads them back from. Everything else is derived.
    let mut current = call.app.plugin_settings.get(&req.id).cloned().unwrap_or_default();
    for (key, value) in &req.settings {
        match toml::Value::try_from(value) {
            Ok(v) => {
                current.insert(key.clone(), v);
            }
            Err(_) => {
                return Err(RpcError::invalid_params(format!(
                    "`{key}` has no TOML form, so it cannot be stored in the config. \
                     Numbers, strings, booleans, arrays and tables all do; null does not."
                )))
            }
        }
    }
    let saved = call
        .app
        .save_plugin_settings(&req.id, current)
        .map_err(|e| RpcError::internal(format!("writing the settings: {e:#}")))?;
    let _ = saved;
    body(settings_of(&call, &installed))
}

fn settings_of(call: &Call, installed: &loader::Installed) -> PluginSettings {
    let mut schemas = Map::new();
    for provide in &installed.manifest.provides {
        let id = format!("{}/{}", installed.name(), provide.id);
        if let Some(schema) = loader::settings_schema(&id) {
            schemas.insert(id, schema);
        }
    }
    let settings = call
        .app
        .plugin_settings
        .get(installed.name())
        .and_then(|t| serde_json::to_value(t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    PluginSettings { name: installed.name().to_string(), settings, schemas }
}
