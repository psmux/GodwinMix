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

use super::{any_object, body, handler};
use crate::control::call::Call;
use godwinmix_core::plugin::loader;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

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
            "Install a plugin, while live, from any source form: a GitHub release \
             (owner/repo), a git URL, cargo:, npm:, pypi:, a local directory, or a bare name \
             looked up in the marketplaces this mixer knows. The signature and the api level \
             are checked before anything is copied.",
            handler(add),
        )
        .params(schema_of::<AddPluginRequest>)
        .result(schema_of::<PluginRecord>)
        .destructive(),
    );

    reg.register(
        MethodDef::new(
            "plugin.update",
            Scope::Admin,
            "Fetch a newer build of a plugin, install it beside the one that is running, and \
             prove it starts. A build that does not answer `initialize` within ten seconds is \
             rolled back and the plugin that was working stays working.",
            handler(update),
        )
        .params(schema_of::<UpdatePluginRequest>)
        .result(schema_of::<PluginUpdated>)
        .destructive(),
    );

    reg.register(
        MethodDef::new(
            "plugin.search",
            Scope::Read,
            "Search every marketplace this mixer knows for a plugin, by name, description or \
             kind. Answers what `gmx plugin add <name>` would install.",
            handler(search),
        )
        .params(schema_of::<SearchRequest>)
        .result(schema_of::<SearchResults>)
        .tool(
            "search_plugins",
            Tier::Search,
            "Find a plugin that is not installed yet. Searches the marketplaces this mixer \
             is configured with, and answers each plugin's name, what it provides, its \
             quality tier and the source to install it from. Use it when an operator asks \
             for a capability this mixer does not have, before saying it cannot be done. \
             Example: search_plugins {term: \"ndi\"}.",
        ),
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
            "tool.call",
            Scope::Operate,
            "Call one of a plugin's tools, in MCP's shape. The name is `<plugin>/<tool>`, or \
             the bare tool name when only one plugin has it.",
            handler(tool_call),
        )
        .params(schema_of::<ToolCallRequest>)
        .result(any_object)
        .not_idempotent()
        .rest_at("POST", "/api/v1/tool/call"),
    );

    reg.register(
        MethodDef::new(
            "device.discover",
            Scope::Operate,
            "Ask every device plugin what it can see: cameras, NDI senders, publishers. Each \
             candidate's params are ready for source.add.",
            handler(discover),
        )
        .params(schema_of::<DiscoverRequest>)
        .result(any_object)
        .tool(
            "discover_sources",
            Tier::Search,
            "Ask every device plugin what it can see right now. Each candidate comes back \
             with a type and params ready to hand to `add_source`, so nobody types an \
             address.",
        ),
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

/// `tool.call`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolCallRequest {
    /// `<plugin>/<tool>`, or the bare tool name when only one plugin has it.
    pub name: String,
    /// The tool's own arguments, as its input schema describes them.
    #[serde(default)]
    pub arguments: Value,
}

/// `device.discover`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct DiscoverRequest {
    /// How long to look, shared between the devices. Two seconds by default,
    /// four and a half at most, because no method blocks for five.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
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
    /// Where the plugin comes from. One of: `owner/repo` (a GitHub release,
    /// optionally `@version`), a git URL ending in `.git`, `cargo:name`,
    /// `npm:@scope/name`, `pypi:name`, `oci:ref`, an absolute path to a
    /// directory, or a bare plugin name to look up in the marketplaces.
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
    /// What was checked about where this came from: "signed", "signed, digest
    /// only", or "custom, unreviewed". 06 section 4: an operator can only
    /// judge a plugin if the catalogue says what was checked.
    pub trust: String,
    /// The sentence behind the label.
    pub trust_detail: String,
    /// Where it was installed from, as it was typed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// Every running instance of it, with what it costs.
    pub instances: Vec<InstanceRecord>,
}

/// `plugin.update`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdatePluginRequest {
    #[serde(alias = "name")]
    pub id: String,
    /// Where the new build comes from. Defaults to wherever this plugin was
    /// installed from last time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// What `plugin.update` answers with.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginUpdated {
    pub from: String,
    pub to: String,
    /// How long the new build took to answer `initialize`.
    pub handshake_ms: u64,
    pub plugin: PluginRecord,
}

/// `plugin.search`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// A word to look for in a plugin's name, description or kind. Empty
    /// lists everything.
    #[serde(default)]
    pub term: String,
}

/// What `plugin.search` answers with.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResults {
    /// The marketplaces that were searched.
    pub marketplaces: Vec<String>,
    pub results: Vec<SearchResult>,
}

/// One plugin a marketplace lists.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    pub name: String,
    pub description: String,
    /// custom, bronze, silver or gold. 06 section 4.
    pub tier: String,
    pub kinds: Vec<String>,
    /// The newest listed version this core's api range can run.
    pub version: String,
    /// What to pass to `plugin.add`.
    pub source: String,
    pub marketplace: String,
    /// Whether it is already on this mixer.
    pub installed: bool,
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
        trust: installed.trust.label().to_string(),
        trust_detail: installed
            .trust
            .explanation()
            .replace("<name>", installed.name()),
        source: installed.trust.origin(),
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
        let mut have: Vec<String> = loader::list().iter().map(|p| p.name().to_string()).collect();
        have.extend(remote_plugins().into_iter().map(|r| r.manifest.plugin.name));
        have.sort();
        have.dedup();
        RpcError::not_found("plugin", name, &have)
    })
}

/// The plugins reachable on a node and not installed here.
///
/// A plugin on a node is not installed on this machine and never will be: the
/// binary is on the other one. It is still a plugin the operator can place a
/// source on, so it is listed, described and counted, with `root` naming the
/// node instead of a directory. A plugin installed here wins the name, because
/// a config written against `ndi/source` should mean the same thing whichever
/// machine ends up running it.
fn remote_plugins() -> Vec<godwinmix_core::plugin::remote::Reachable> {
    godwinmix_core::plugin::remote::plugins()
        .into_iter()
        .filter(|r| loader::get(&r.manifest.plugin.name).is_none())
        .collect()
}

/// One reachable plugin, in the shape `plugin.list` answers with.
fn remote_record(reachable: &godwinmix_core::plugin::remote::Reachable) -> PluginRecord {
    let manifest = &reachable.manifest;
    let name = manifest.plugin.name.clone();
    let provides: Vec<String> =
        manifest.provides.iter().map(|p| format!("{name}/{}", p.id)).collect();
    let instances = loader::stats()
        .into_iter()
        .filter(|s| s.plugin == name)
        .map(instance)
        .collect();
    PluginRecord {
        name: name.clone(),
        version: manifest.plugin.version.clone(),
        description: manifest.plugin.description.clone(),
        enabled: true,
        root: format!("node:{}", reachable.node),
        provides,
        tools: manifest.tools.iter().map(|t| format!("gmx_{name}_{}", t.name)).collect(),
        hooks: Vec::new(),
        problem: None,
        trust: "node".into(),
        trust_detail: format!(
            "installed on the node `{}`, which this core trusts because it holds a certificate \
             this core signed",
            reachable.node
        ),
        source: format!("node:{}", reachable.node),
        instances,
    }
}

async fn list(_call: Call, _params: Value) -> Result<Value, RpcError> {
    let mut plugins: Vec<PluginRecord> = loader::list().iter().map(record).collect();
    plugins.extend(remote_plugins().iter().map(remote_record));
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    body(PluginListing {
        plugins,
        plugins_dir: loader::dir().to_string_lossy().into_owned(),
    })
}

async fn stats(_call: Call, _params: Value) -> Result<Value, RpcError> {
    body(StatsListing { instances: loader::stats().into_iter().map(instance).collect() })
}

async fn describe(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PluginName = call.params(&params)?;
    // A plugin that is only on a node is described from the manifest and the
    // schemas that node sent at its hello, in exactly the shape a local one
    // is. Same fields, same settings form, same tool list.
    if loader::get(&req.id).is_none() {
        if let Some(reachable) = remote_plugins()
            .into_iter()
            .find(|r| r.manifest.plugin.name == req.id)
        {
            return describe_remote(&reachable);
        }
    }
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
    let source = godwinmix_host::sources::Source::parse(&req.source)
        .or_else(|direct| {
            // Not a source form. It may still be a name a marketplace knows,
            // which is what the docs teach; the loader resolves that. Anything
            // else keeps the parse error, which lists every form.
            if godwinmix_host::marketplace::resolve(&req.source, &options(&call).only).is_some() {
                Ok(godwinmix_host::sources::Source::Path(std::path::PathBuf::new()))
            } else {
                Err(direct)
            }
        })
        .map_err(|e| {
            RpcError::new(ErrorCode::NotFound, format!("{e:#}")).with("source", req.source.clone())
        })?;
    if let godwinmix_host::sources::Source::Path(path) = &source {
        if !path.as_os_str().is_empty() && !path.exists() {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                format!(
                    "there is nothing at `{}`. A path source is a directory with \
                     gmx-plugin.toml at its root; `gmx plugin new` writes one.",
                    req.source
                ),
            )
            .with("source", req.source));
        }
    }
    if call.dry_run {
        return Ok(call.dry_run_answer(true, vec![dry_run_line(&req.source)]));
    }
    // A local copy of a directory and a manifest parse is usually quick, and a
    // release to download, a venv to build or a crate to compile is not. So
    // this answers with a task handle either way, which is what 03 section 6
    // asks of every method that might take longer than five seconds: a
    // client's own timeout then never leaves the work in an unknown state.
    let spec = req.source.clone();
    let opts = options(&call);
    let hooks = call.app.hooks.clone();
    let supervisor = call.app.plugins.clone();
    Ok(super::tasks::spawn_task(
        &call.app.tasks,
        "plugin.add",
        Some(serde_json::json!({ "source": req.source })),
        move |_ctx| async move {
            let installed = tokio::task::spawn_blocking(move || loader::install(&spec, &opts))
                .await
                .map_err(|e| format!("the install task did not finish: {e}"))?
                .map_err(|e| format!("{e:#}"))?;
            // The hook call site: take on whatever the new plugin asked for,
            // and tell everyone else it arrived.
            plugin_arrived(&hooks, &installed);
            // A service, a device or a transition is one instance per plugin
            // and starts with the core, so a plugin installed while the core
            // is running starts now rather than at the next restart. Anything
            // already running is left alone.
            let failures = tokio::task::spawn_blocking(move || supervisor.start_all())
                .await
                .unwrap_or_default();
            let mut answer =
                serde_json::to_value(record(&installed)).map_err(|e| e.to_string())?;
            if let (Some(map), false) = (answer.as_object_mut(), failures.is_empty()) {
                map.insert(
                    "problems".into(),
                    json!(failures
                        .iter()
                        .map(|(provide, why)| format!("{provide}: {why}"))
                        .collect::<Vec<_>>()),
                );
            }
            Ok(answer)
        },
    ))
}

/// One line saying what an install would do, for `--dry-run`.
fn dry_run_line(spec: &str) -> String {
    let path = std::path::Path::new(spec);
    if path.join("gmx-plugin.toml").is_file() {
        if let Ok(manifest) =
            godwinmix_protocol::plugin::manifest::Manifest::load(path.join("gmx-plugin.toml"))
        {
            return format!(
                "install {} v{} from {spec}, adding {} provide(s)",
                manifest.plugin.name,
                manifest.plugin.version,
                manifest.provides.len()
            );
        }
    }
    format!(
        "fetch {spec}, check its signature and its api level, and install it under the \
         plugins directory"
    )
}

async fn update(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: UpdatePluginRequest = call.params(&params)?;
    let installed = find(&req.id)?;
    // Where it came from last time, when the caller did not say.
    let spec = req
        .source
        .clone()
        .or_else(|| {
            Some(installed.trust.source.clone()).filter(|s| !s.trim().is_empty())
        })
        .ok_or_else(|| {
            RpcError::invalid_params(format!(
                "`{}` has no record of where it was installed from, so there is nothing to \
                 update it from. Say where: `gmx plugin update {} <source>`.",
                req.id, req.id
            ))
        })?;
    if call.dry_run {
        return Ok(call.dry_run_answer(
            true,
            vec![format!(
                "fetch {spec}, install it beside {} v{}, and roll back if it does not start",
                installed.name(),
                installed.version()
            )],
        ));
    }
    let name = req.id.clone();
    let opts = options(&call);
    Ok(super::tasks::spawn_task(
        &call.app.tasks,
        "plugin.update",
        Some(serde_json::json!({ "plugin": req.id, "source": spec })),
        move |_ctx| async move {
            let updated =
                tokio::task::spawn_blocking(move || loader::update(&name, &spec, &opts))
                    .await
                    .map_err(|e| format!("the update task did not finish: {e}"))?
                    .map_err(|e| format!("{e:#}"))?;
            serde_json::to_value(PluginUpdated {
                from: updated.from,
                to: updated.to,
                handshake_ms: updated.handshake_ms as u64,
                plugin: record(&updated.installed),
            })
            .map_err(|e| e.to_string())
        },
    ))
}

async fn search(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SearchRequest = call.params(&params).unwrap_or(SearchRequest { term: String::new() });
    let only = options(&call).only;
    let found = godwinmix_host::marketplace::search(&req.term, &only);
    let installed: Vec<String> = loader::list().iter().map(|p| p.name().to_string()).collect();
    body(SearchResults {
        marketplaces: godwinmix_host::marketplace::documents(&only)
            .into_iter()
            .map(|m| m.name)
            .collect(),
        results: found
            .into_iter()
            .map(|(market, listing)| SearchResult {
                installed: installed.contains(&listing.name),
                marketplace: market,
                description: listing.description.clone(),
                tier: listing.tier.to_string(),
                kinds: listing.kinds.clone(),
                version: listing
                    .usable_version()
                    .map(|v| v.version.clone())
                    .unwrap_or_default(),
                source: listing.source.clone(),
                name: listing.name,
            })
            .collect(),
    })
}

/// What the operator's config says an install may do.
fn options(call: &Call) -> loader::InstallOptions {
    loader::InstallOptions {
        allow_unsigned: call.app.allow_unsigned,
        only: call.app.marketplaces_only.clone(),
        offline: false,
    }
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
    // Every process this plugin has running stops before its directory goes,
    // or `plugin.add` then `plugin.remove` would leave a process holding a
    // deleted binary. The leak test counts exactly that.
    let supervisor = call.app.plugins.clone();
    let name = req.id.clone();
    let _ = tokio::task::spawn_blocking(move || {
        supervisor.stop_plugin(&name, "the plugin was removed")
    })
    .await;
    let gone = loader::uninstall(&req.id)
        .map_err(|e| RpcError::new(ErrorCode::NotInState, format!("{e:#}")))?;
    // Its credentials and its secrets go with it. A token minted for an
    // instance of a plugin that is no longer installed is a key to a door that
    // has been bricked up, and leaving one lying about is how a revoked plugin
    // keeps calling.
    let revoked = call.app.tokens.revoke_plugin(gone.name());
    let forgotten = secrets().forget(gone.name());
    if revoked > 0 || forgotten > 0 {
        tracing::info!(
            plugin = gone.name(),
            revoked,
            forgotten,
            "revoked a removed plugin's tokens and forgot its secrets"
        );
    }
    // The hook call site: everything it registered goes with it, then the
    // rest of the world is told.
    call.app.hooks.unregister_plugin(gone.name());
    call.app.hooks.fire(godwinmix_core::hooks::name::PLUGIN_STATE, || {
        serde_json::json!({ "plugin": gone.name(), "state": "stopped", "detail": "removed" })
    });
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
    // Enabling starts this plugin's singletons; disabling stops them and lets
    // go of anything a device of its added. Sources are the operator's and are
    // left where they are either way: disabling a plugin is not a reason to
    // cut a camera somebody put on air.
    let supervisor = call.app.plugins.clone();
    let name = req.id.clone();
    let _ = tokio::task::spawn_blocking(move || match on {
        true => {
            supervisor.start_all();
        }
        false => {
            supervisor.stop_plugin(&name, "the plugin was disabled");
        }
    })
    .await;
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
    // Now swap what is running. One instance at a time, each new one built
    // before the old one is stopped, and a handshake that fails puts the
    // previous instance back rather than leaving a hole. A source belonging to
    // this plugin keeps its slot and its last frame throughout, because the
    // mixer's freeze frame covers a source being rebuilt.
    let plugin = req.id.clone();
    let supervisor = call.app.plugins.clone();
    let swapped = tokio::task::spawn_blocking(move || supervisor.reload(&plugin))
        .await
        .map_err(|e| {
            RpcError::new(ErrorCode::InternalError, format!("the reload could not be run: {e}"))
        })?;
    if let Some((instance, why)) = swapped.failed.first() {
        return Err(RpcError::new(
            ErrorCode::NotInState,
            format!(
                "{} was read again, but `{instance}` would not start on the new version, so \
                 the previous one is running: {why}. Fix the plugin and call plugin.reload \
                 again; nothing was left stopped.",
                req.id
            ),
        )
        .with("instance", instance.clone())
        .with("swapped", json!(swapped.swapped))
        .with("retryable", true));
    }
    let mut answer = serde_json::to_value(record(&fresh)).map_err(|e| {
        RpcError::new(ErrorCode::InternalError, format!("encoding the plugin record: {e}"))
    })?;
    if let Some(map) = answer.as_object_mut() {
        map.insert("reloaded".into(), json!(swapped.swapped));
    }
    body(answer)
}

async fn tool_call(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ToolCallRequest = call.params(&params)?;
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(RpcError::invalid_params(
            "tool.call takes the name of a tool, as `<plugin>/<tool>` or, when only one \
             plugin has it, the bare tool name. `plugin.describe` lists what each plugin \
             offers and `search_tools` finds one by what it does."
                .to_string(),
        ));
    }
    // 03 section 6 gives a plugin's own token the scope `plugin:<name>`: it
    // may call its own tools and nobody else's. An operator's token carries no
    // plugin and reaches every tool, which is what it always did. The method
    // is registered at `Scope::Plugin`, which an operate or admin token also
    // satisfies and a read only token does not.
    let supervisor = call.app.plugins.clone();
    if let Some(mine) = call.token.plugin.clone() {
        let owner = supervisor.tool_owner(&name);
        if !owner.as_deref().is_some_and(|o| o == mine) {
            return Err(RpcError::new(
                ErrorCode::Scope,
                format!(
                    "this token belongs to the plugin `{mine}`, and `{name}` is {}. A plugin's \
                     own token reaches its own tools and nothing else",
                    match owner {
                        Some(other) => format!("a tool of `{other}`"),
                        None => "not one of its tools".to_string(),
                    }
                ),
            ));
        }
    }
    let arguments = req.arguments.clone();
    let answered = tokio::task::spawn_blocking(move || supervisor.tool_call(&name, arguments))
        .await
        .map_err(|e| RpcError::new(ErrorCode::InternalError, format!("the tool call panicked: {e}")))?;
    match answered {
        Ok(value) => body(value),
        Err(e) => Err(RpcError::new(ErrorCode::NotFound, format!("{e:#}"))),
    }
}

async fn discover(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: DiscoverRequest = call.params(&params)?;
    let within = std::time::Duration::from_millis(req.timeout_ms.unwrap_or(2_000).min(4_500));
    let supervisor = call.app.plugins.clone();
    let found = tokio::task::spawn_blocking(move || supervisor.discover(within))
        .await
        .map_err(|e| RpcError::new(ErrorCode::InternalError, format!("discovery panicked: {e}")))?;
    body(json!({ "candidates": found }))
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
    // Secrets never reach the config file. A field the schema marks
    // `"format": "secret"` is taken out here, sealed under the store's own
    // key, and what is left is what gets written down. A field carrying the
    // sentinel is a form handing back what it was given and means "unchanged".
    let secret_fields = secret_fields_of(&installed);
    if !secret_fields.is_empty() {
        let mut incoming: godwinmix_core::config::Params = Default::default();
        for (key, value) in &req.settings {
            if let Ok(v) = toml::Value::try_from(value) {
                incoming.insert(key.clone(), v);
            }
        }
        for (field, value) in godwinmix_core::secrets::take(&mut incoming, &secret_fields) {
            secrets()
                .set(&req.id, &field, &value)
                .map_err(|e| RpcError::internal(format!("sealing `{field}`: {e:#}")))?;
        }
        for field in &secret_fields {
            current.remove(field);
        }
    }
    for (key, value) in &req.settings {
        if secret_fields.iter().any(|f| f == key) {
            continue;
        }
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

/// `plugin.describe` for a plugin that lives on a node.
fn describe_remote(
    reachable: &godwinmix_core::plugin::remote::Reachable,
) -> Result<Value, RpcError> {
    let manifest = serde_json::to_value(&reachable.manifest)
        .map_err(|e| RpcError::internal(format!("encoding the manifest: {e}")))?;
    let mut schemas = Map::new();
    for provide in &reachable.manifest.provides {
        let id = format!("{}/{}", reachable.manifest.plugin.name, provide.id);
        if let Some(schema) = godwinmix_core::plugin::remote::schema(&id) {
            schemas.insert(id, schema);
        }
    }
    // Skills are files on the node's disk and are not carried across. A
    // reader who wants one reads it on the machine it is on; saying so beats
    // an empty map that looks like the plugin has none.
    body(PluginDescription {
        plugin: remote_record(reachable),
        manifest,
        schemas,
        skills: Map::new(),
    })
}

fn settings_of(call: &Call, installed: &loader::Installed) -> PluginSettings {
    let mut schemas = Map::new();
    for provide in &installed.manifest.provides {
        let id = format!("{}/{}", installed.name(), provide.id);
        if let Some(schema) = loader::settings_schema(&id) {
            schemas.insert(id, schema);
        }
    }
    let mut table = call.app.plugin_settings.get(installed.name()).cloned().unwrap_or_default();
    // What a surface sees where a secret is: the sentinel when one is stored,
    // and nothing when one is not. The value itself never leaves the store.
    let fields = secret_fields_of(installed);
    if !fields.is_empty() {
        let store = secrets();
        let name = installed.name().to_string();
        godwinmix_core::secrets::hide(&mut table, &fields, |field| store.has(&name, field));
    }
    let settings = serde_json::to_value(&table)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    PluginSettings { name: installed.name().to_string(), settings, schemas }
}

/// Which of a plugin's settings fields the schema marks as secret.
///
/// Across every provide, because `[plugin] settings` are per plugin and a
/// stream key declared on one provide is the same secret whichever of them
/// reads it.
fn secret_fields_of(installed: &loader::Installed) -> Vec<String> {
    let mut fields = Vec::new();
    for provide in &installed.manifest.provides {
        let id = format!("{}/{}", installed.name(), provide.id);
        if let Some(schema) = loader::settings_schema(&id) {
            fields.extend(godwinmix_core::secrets::secret_fields(&schema));
        }
    }
    fields.sort();
    fields.dedup();
    fields
}

/// The secret store, opened once under the plugins directory's parent.
///
/// `~/.godwinmix/secrets`, beside the plugins themselves. A process global
/// because `settings_of` is called from a handler that has no place to keep
/// one, and because there is exactly one of these per machine.
fn secrets() -> &'static godwinmix_core::secrets::Secrets {
    static STORE: std::sync::OnceLock<godwinmix_core::secrets::Secrets> =
        std::sync::OnceLock::new();
    STORE.get_or_init(|| {
        let dir = godwinmix_host::marketplace::home_dir().join("secrets");
        godwinmix_core::secrets::Secrets::open(&dir).unwrap_or_else(|e| {
            // A store that will not open is a machine problem, not a reason to
            // refuse to start. Secrets are then not stored and the settings
            // form says a field is empty, which is true.
            tracing::error!(error = %format!("{e:#}"), "no secret store; fields marked secret will not persist");
            godwinmix_core::secrets::Secrets::open(&std::env::temp_dir().join("gmx-secrets"))
                .expect("a temporary secret store")
        })
    })
}

/// A plugin has just landed: register its hooks, then say so.
///
/// `plugin.loaded` and `plugin.failed` are the two halves of the same moment,
/// and `plugin.state` carries the lifecycle enum for anything that wants one
/// name for both.
fn plugin_arrived(hooks: &std::sync::Arc<crate::control::hooks::Hooks>, installed: &loader::Installed) {
    use godwinmix_core::hooks::name;
    let name_of = installed.name().to_string();
    match &installed.problem {
        None => {
            hooks.register_plugin(&name_of, &installed.manifest.hooks);
            let version = installed.version().to_string();
            let provides = installed.provides.clone();
            hooks.fire(name::PLUGIN_LOADED, || {
                serde_json::json!({ "plugin": name_of, "version": version, "provides": provides })
            });
        }
        Some(problem) => {
            let problem = problem.clone();
            hooks.fire(name::PLUGIN_FAILED, || {
                serde_json::json!({ "plugin": name_of, "reason": problem })
            });
        }
    }
    let state = if installed.problem.is_none() { "ready" } else { "failed" };
    let named = installed.name().to_string();
    hooks.fire(godwinmix_core::hooks::name::PLUGIN_STATE, || {
        serde_json::json!({ "plugin": named, "state": state })
    });
}

