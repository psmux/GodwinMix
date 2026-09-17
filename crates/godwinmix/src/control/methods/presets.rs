//! `preset.list`, `preset.apply` and `preset.save`.
//!
//! The same three things `gmx preset` does, over the protocol, because principle
//! 3 says the first party UI uses the public contract and nothing else. The
//! welcome panel in the web UI is a client of these and of nothing private.
//!
//! Applying a preset to a running core writes the files and then reloads what
//! can be reloaded live: the sources and outputs it brought are added through
//! the same commands `source.add` and `output.add` use, and the layout, theme
//! and gallery mode go out as `event/ui.changed` for every client to pick up.
//! Nothing here restarts the encoder and nothing here touches the programme.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::preset::{self, plan::Options, Next};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::UiDefaults;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use parking_lot::RwLock;

/// The config this core was started with, and the surface defaults in force.
///
/// Held here rather than on `AppState` so that adding presets costs the shared
/// control plane one call at startup and no new field. `configure` is called
/// once from `run`; everything else reads.
struct Runtime {
    config_path: PathBuf,
    ui: UiDefaults,
}

static RUNTIME: OnceLock<RwLock<Runtime>> = OnceLock::new();

fn runtime() -> &'static RwLock<Runtime> {
    RUNTIME.get_or_init(|| {
        RwLock::new(Runtime { config_path: PathBuf::from("godwinmix.toml"), ui: UiDefaults::default() })
    })
}

/// Called once at startup with the config in force and its `[ui]` section.
pub fn configure(config_path: &Path, ui: UiDefaults) {
    let mut held = runtime().write();
    held.config_path = config_path.to_path_buf();
    held.ui = ui;
}

/// What a surface should start with. `core.info` answers with this.
///
/// A core no preset has been applied to still answers with a gallery mode: the
/// one `gmx doctor` proposes from this machine's cores and memory, so a client
/// on a Pi starts on icons rather than guessing from `hardwareConcurrency`.
/// `preset` is absent there, which is what tells a surface that nobody has set
/// this core up yet.
pub fn ui_defaults() -> Option<UiDefaults> {
    let ui = runtime().read().ui.clone();
    if !ui.is_empty() {
        return Some(ui);
    }
    Some(UiDefaults {
        gallery: Some(godwinmix_core::observe::doctor::gallery_default_here().to_string()),
        ..UiDefaults::default()
    })
}

fn config_path() -> PathBuf {
    runtime().read().config_path.clone()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ApplyRequest {
    /// A preset name, or a path to a directory holding `gmx-plugin.toml`.
    pub name: String,
    /// Work out the plan and write nothing.
    #[serde(default)]
    pub dry_run: bool,
    /// Take the preset's value wherever the operator already has one.
    #[serde(default)]
    pub force: bool,
    /// Leave the operator's sources and outputs alone.
    #[serde(default)]
    pub keep_sources: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveRequest {
    /// The new preset's name. A slug: lower case letters, digits and hyphens.
    pub name: String,
    /// Where to write it. Defaults to `~/.godwinmix/presets/<name>`.
    #[serde(default)]
    pub out: Option<String>,
}

/// What `preset.apply` answers with.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ApplyResult {
    /// True when nothing was written because `dry_run` was set.
    pub dry_run: bool,
    /// The whole plan, as JSON. The same object `preset.list` rows point at.
    pub plan: Value,
    /// Present when the preset was actually applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<Value>,
    /// The sources and outputs this core picked up without a restart.
    pub live: Vec<String>,
    /// What the person does next, typed, in the order they do it. The same
    /// list as `plan.next`, lifted out so a surface does not have to dig for
    /// the one field it builds its checklist from.
    pub next: Vec<Next>,
    /// What the preset brought that is not running yet, one entry each.
    /// Empty is the good case.
    pub needs_restart: Vec<Pending>,
}

/// One thing the preset brought that this core is not running, and why.
///
/// `reason` is what a surface branches on and `message` is what a terminal
/// prints. Three reasons, and they must not be blurred together: a thing
/// waiting on a plugin can be fixed from the page, a thing that was refused
/// wants looking at now, and a restart is a thing that will be fine.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct Pending {
    /// The source or output id, or the config path for the keys that were
    /// written.
    pub id: String,
    /// `plugin_missing`, `refused` or `restart`.
    pub reason: String,
    /// The plugin to install, on `plugin_missing`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// The same sentence the CLI prints.
    pub message: String,
}

impl Pending {
    fn new(id: impl Into<String>, reason: &str, message: impl Into<String>) -> Self {
        Self { id: id.into(), reason: reason.into(), plugin: None, message: message.into() }
    }
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "preset.list",
            Scope::Read,
            "Every preset this core can apply: the six built in, plus anything installed \
             beside the binary or under ~/.godwinmix/presets.",
            handler(|_call: Call, _| async move { body(preset::list()) }),
        )
        .result(any_object)
        .tool(
            "list_presets",
            Tier::Search,
            "The named setups this mixer can apply in one step: for a church service, a \
             classroom, an esports match, a headless channel an agent drives, a broadcast \
             contribution feed, and the default. Each row says what it is for, which \
             plugins it needs, which theme and gallery mode it chooses, and what is left \
             for the person afterwards, both as typed `next` entries and as sentences. \
             Use it before `apply_preset`.",
        ),
    );

    reg.register(
        MethodDef::new(
            "preset.apply",
            Scope::Admin,
            "Put a preset on this core: its config, its scenes, its layout, its theme and \
             its gallery mode. Pass dry_run to get the plan and write nothing.",
            handler(apply),
        )
        .params(schema_of::<ApplyRequest>)
        .result(schema_of::<ApplyResult>)
        .destructive()
        .tool(
            "apply_preset",
            Tier::Search,
            "Configure this mixer from a named preset in one step: it merges the preset's \
             configuration into the operator's (their own values win unless force is set), \
             writes the preset's scenes, and sets the UI layout, theme and gallery mode. \
             Sources and outputs are appended by id and never duplicated, so applying the \
             same preset twice changes nothing the second time. Always call it with \
             dry_run true first and read the plan: it names every plugin that is not \
             installed and every placeholder a person still has to fill in. Admin scope, \
             and it rewrites the operator's configuration file, so it is not something to \
             do during a show.",
        ),
    );

    reg.register(
        MethodDef::new(
            "preset.save",
            Scope::Admin,
            "Turn this core's working setup into a preset directory somebody else can \
             apply. Stream keys and the control token are replaced with placeholders.",
            handler(save),
        )
        .params(schema_of::<SaveRequest>)
        .result(any_object),
    );
}

async fn apply(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ApplyRequest = call.params(&params)?;
    let found = preset::resolve(&req.name).map_err(|e| {
        RpcError::new(ErrorCode::NotFound, format!("{e:#}")).with("preset", req.name.clone())
    })?;
    let options = Options {
        config_path: config_path(),
        force: req.force,
        keep_sources: req.keep_sources,
    };
    let plan = preset::plan::build(&found, &options).map_err(|e| {
        RpcError::invalid_params(format!("the preset {} does not apply here: {e:#}", req.name))
    })?;
    let plan_json = serde_json::to_value(&plan)
        .map_err(|e| RpcError::internal(format!("encoding the plan: {e}")))?;

    if req.dry_run || call.dry_run {
        return body(ApplyResult {
            dry_run: true,
            plan: plan_json,
            applied: None,
            live: Vec::new(),
            next: plan.next.clone(),
            needs_restart: Vec::new(),
        });
    }

    let applied = preset::apply::run(&found, &plan)
        .map_err(|e| RpcError::internal(format!("applying {}: {e:#}", req.name)))?;
    let reloaded = reload(&call, &plan).await;
    let needs_restart = still_pending(&plan, &reloaded);
    let live = reloaded.live.clone();

    {
        let mut held = runtime().write();
        held.ui = applied.ui.clone();
    }
    call.app.mixer.emit(godwinmix_protocol::types::Event::UiChanged { ui: applied.ui.clone() });

    let applied_json = serde_json::to_value(&applied)
        .map_err(|e| RpcError::internal(format!("encoding the result: {e}")))?;
    body(ApplyResult {
        dry_run: false,
        plan: plan_json,
        applied: Some(applied_json),
        live,
        next: plan.next.clone(),
        needs_restart,
    })
}

/// What a running core did with the preset: what it took, and what it tried
/// and could not.
///
/// The two are kept apart because they read differently to the operator. A
/// thing waiting for a restart is a thing that will be fine. A thing that
/// refused to start is a thing to look at now.
#[derive(Debug, Default)]
pub struct Reload {
    /// The sources and outputs this core picked up, as `source <id>` and
    /// `output <id>`.
    pub live: Vec<String>,
    /// The ones whose add was refused, each with what the core said.
    pub failed: Vec<(String, String)>,
}

/// Add what a running core can take now: the sources and outputs the preset
/// brought whose plugin is here.
///
/// Everything else stays in the file and comes up on the next start. Nothing in
/// here can fail the call: what would not start is reported, not thrown.
async fn reload(call: &Call, plan: &preset::Plan) -> Reload {
    use godwinmix_core::mixer::Command;

    let mut out = Reload::default();
    let Ok(config) = godwinmix_core::config::Config::load(&godwinmix_core::config::path_in_force(
        &plan.config_path,
    )) else {
        return out;
    };
    let status = match call.app.mixer.status().await {
        Ok(s) => s,
        Err(_) => return out,
    };

    for source in &config.sources {
        if status.sources.iter().any(|s| s.id == source.id) {
            continue;
        }
        if plan.sources.iter().any(|a| a.id == source.id && a.needs_plugin.is_some()) {
            continue;
        }
        let cfg = source.clone();
        match call
            .app
            .mixer
            .request(|ack| Command::AddSource(Box::new(cfg), Some(ack)))
            .await
        {
            Ok(_) => out.live.push(format!("source {}", source.id)),
            Err(e) => out.failed.push((source.id.clone(), e.to_string())),
        }
    }
    for output in &config.outputs {
        if status.outputs.iter().any(|o| o.id == output.id) {
            continue;
        }
        if plan.outputs.iter().any(|a| a.id == output.id && a.needs_plugin.is_some()) {
            continue;
        }
        if call.app.rehearsal {
            continue;
        }
        let cfg = output.clone();
        match call
            .app
            .mixer
            .request(|ack| Command::AddOutput(Box::new(cfg), Some(ack)))
            .await
        {
            Ok(_) => out.live.push(format!("output {}", output.id)),
            Err(e) => out.failed.push((output.id.clone(), e.to_string())),
        }
    }
    out
}

/// What the preset brought that this core did not pick up, and why.
///
/// Three different things end up here and they must not be blurred together:
/// something waiting on a plugin, something that was refused when this core
/// tried it, and something the core never tried and will take on the next
/// start. Only the last of those is a restart.
fn still_pending(plan: &preset::Plan, reload: &Reload) -> Vec<Pending> {
    let mut out = Vec::new();
    for addition in plan.sources.iter().chain(&plan.outputs) {
        // An exact match on the label `reload` wrote, so that an id which is
        // the tail of another id cannot be mistaken for it.
        let took = reload
            .live
            .iter()
            .any(|l| l == &format!("source {}", addition.id) || l == &format!("output {}", addition.id));
        if addition.already_there || took {
            continue;
        }
        if let Some((_, why)) = reload.failed.iter().find(|(id, _)| id == &addition.id) {
            out.push(Pending::new(
                &addition.id,
                "refused",
                format!("{} did not start: {why}", addition.id),
            ));
            continue;
        }
        match &addition.needs_plugin {
            Some(plugin) => {
                let mut item = Pending::new(
                    &addition.id,
                    "plugin_missing",
                    format!(
                        "{} waits for the {plugin} plugin: `gmx plugin add {plugin}`",
                        addition.id
                    ),
                );
                item.plugin = Some(plugin.clone());
                out.push(item);
            }
            None => out.push(Pending::new(
                &addition.id,
                "restart",
                format!(
                    "{} is in the config file; this core did not bring it up now, so it \
                     starts on the next `gmx` restart",
                    addition.id
                ),
            )),
        }
    }
    if !plan.config.is_empty() {
        out.push(Pending::new(
            plan.config_path.display().to_string(),
            "restart",
            format!(
                "{} configuration key(s) were written to {} and take effect on restart",
                plan.config.len(),
                plan.config_path.display()
            ),
        ));
    }
    out
}

async fn save(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SaveRequest = call.params(&params)?;
    let out = req
        .out
        .map(PathBuf::from)
        .unwrap_or_else(|| preset::save::default_dir(&req.name));
    let saved = preset::save::run(&req.name, &config_path(), &out)
        .map_err(|e| RpcError::invalid_params(format!("{e:#}")))?;
    body(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_absent_until_a_preset_sets_them() {
        // A fresh core answers core.info with no `ui`, which is what puts the
        // welcome panel up in the reference UI.
        assert!(UiDefaults::default().is_empty());
    }

    #[test]
    fn the_church_preset_answers_with_a_typed_list_the_page_can_draw() {
        let _ = gstreamer::init();
        let found = preset::resolve("church").unwrap();
        let plan =
            preset::plan::build(&found, &Options::new("/nowhere/godwinmix.toml")).unwrap();
        // Two stream keys, the camera plugin, and the first take. Nothing in
        // it is a sentence telling somebody to open a file.
        let actions: Vec<&str> = plan.next.iter().map(|n| n.action.as_str()).collect();
        assert_eq!(actions, ["stream_key", "stream_key", "install_plugin", "take"]);
        let outputs: Vec<&str> =
            plan.next.iter().filter_map(|n| n.output.as_deref()).collect();
        assert_eq!(outputs, ["youtube", "facebook"]);
        for step in &plan.steps {
            assert!(!step.contains("godwinmix.toml"), "{step:?} sends somebody to a text editor");
        }
    }

    #[test]
    fn what_is_left_after_an_apply_names_the_plugin_and_the_restart() {
        let _ = gstreamer::init();
        let found = preset::resolve("church").unwrap();
        let plan =
            preset::plan::build(&found, &Options::new("/nowhere/godwinmix.toml")).unwrap();
        let pending = still_pending(&plan, &Reload::default());
        // Every source the church preset brings runs on a built in kind, so
        // each one is waiting on a restart and none on a plugin.
        assert!(pending.iter().any(|p| p.id == "cam-wide"), "{pending:?}");
        assert!(pending.iter().all(|p| p.reason != "plugin_missing"), "{pending:?}");
        assert!(pending.iter().any(|p| p.reason == "restart"), "{pending:?}");
        assert!(pending.iter().all(|p| !p.message.is_empty()), "the prose is still there");
        // The camera plugin is still named, on the plan rather than here.
        assert!(plan.missing().iter().any(|p| p.name == "camera"));
    }

    #[test]
    fn a_source_that_was_refused_is_not_reported_as_waiting_for_a_restart() {
        let _ = gstreamer::init();
        let found = preset::resolve("church").unwrap();
        let plan =
            preset::plan::build(&found, &Options::new("/nowhere/godwinmix.toml")).unwrap();
        let id = plan.sources.first().expect("the church preset brings sources").id.clone();
        let reloaded = Reload {
            live: Vec::new(),
            failed: vec![(id.clone(), "no such device".into())],
        };
        let pending = still_pending(&plan, &reloaded);
        let line = pending
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("nothing about {id} in {pending:?}"));
        assert_eq!(line.reason, "refused");
        assert!(line.message.contains("no such device"), "{}", line.message);
        assert!(!line.message.contains("restart"), "{}", line.message);
    }

    #[test]
    fn an_id_that_is_the_tail_of_another_is_not_mistaken_for_it() {
        let _ = gstreamer::init();
        let found = preset::resolve("church").unwrap();
        let plan =
            preset::plan::build(&found, &Options::new("/nowhere/godwinmix.toml")).unwrap();
        let id = plan.sources.first().expect("the church preset brings sources").id.clone();
        // A different source whose label ends with this id. The old suffix
        // match read this as "the one we wanted came up".
        let reloaded = Reload {
            live: vec![format!("source extra-{id}")],
            failed: Vec::new(),
        };
        let pending = still_pending(&plan, &reloaded);
        assert!(pending.iter().any(|p| p.id == id), "{pending:?}");
    }
}
