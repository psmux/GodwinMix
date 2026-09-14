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
use godwinmix_core::preset::{self, plan::Options};
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
pub fn ui_defaults() -> Option<UiDefaults> {
    let ui = runtime().read().ui.clone();
    (!ui.is_empty()).then_some(ui)
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
    /// What still needs a restart, in plain words. Empty is the good case.
    pub needs_restart: Vec<String>,
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
             plugins it needs, which theme and gallery mode it chooses, and the three \
             steps left for the person afterwards. Use it before `apply_preset`.",
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
            needs_restart: Vec::new(),
        });
    }

    let applied = preset::apply::run(&found, &plan)
        .map_err(|e| RpcError::internal(format!("applying {}: {e:#}", req.name)))?;
    let live = reload(&call, &plan).await;
    let needs_restart = still_pending(&plan, &live);

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
        needs_restart,
    })
}

/// Add what a running core can take now: the sources and outputs the preset
/// brought whose plugin is here.
///
/// Everything else stays in the file and comes up on the next start. Nothing in
/// here can fail the call: a source that will not start is a source the operator
/// sees in `source.list` with its state, which is where it belongs.
async fn reload(call: &Call, plan: &preset::Plan) -> Vec<String> {
    use godwinmix_core::mixer::Command;

    let mut live = Vec::new();
    let Ok(config) = godwinmix_core::config::Config::load(&godwinmix_core::config::path_in_force(
        &plan.config_path,
    )) else {
        return live;
    };
    let status = match call.app.mixer.status().await {
        Ok(s) => s,
        Err(_) => return live,
    };

    for source in &config.sources {
        if status.sources.iter().any(|s| s.id == source.id) {
            continue;
        }
        if plan.sources.iter().any(|a| a.id == source.id && a.needs_plugin.is_some()) {
            continue;
        }
        let cfg = source.clone();
        if call
            .app
            .mixer
            .request(|ack| Command::AddSource(Box::new(cfg), Some(ack)))
            .await
            .is_ok()
        {
            live.push(format!("source {}", source.id));
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
        if call
            .app
            .mixer
            .request(|ack| Command::AddOutput(Box::new(cfg), Some(ack)))
            .await
            .is_ok()
        {
            live.push(format!("output {}", output.id));
        }
    }
    live
}

/// What the preset brought that this core did not pick up, and why.
fn still_pending(plan: &preset::Plan, live: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for addition in plan.sources.iter().chain(&plan.outputs) {
        if addition.already_there || live.iter().any(|l| l.ends_with(&addition.id)) {
            continue;
        }
        match &addition.needs_plugin {
            Some(plugin) => out.push(format!(
                "{} waits for the {plugin} plugin: `gmx plugin add {plugin}`",
                addition.id
            )),
            None => out.push(format!(
                "{} is in the config file; this core did not bring it up now, so it \
                 starts on the next `gmx` restart",
                addition.id
            )),
        }
    }
    if !plan.config.is_empty() {
        out.push(format!(
            "{} configuration key(s) were written to {} and take effect on restart",
            plan.config.len(),
            plan.config_path.display()
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
    fn what_is_left_after_an_apply_names_the_plugin_and_the_restart() {
        let _ = gstreamer::init();
        let found = preset::resolve("church").unwrap();
        let plan =
            preset::plan::build(&found, &Options::new("/nowhere/godwinmix.toml")).unwrap();
        let pending = still_pending(&plan, &[]);
        // Every source the church preset brings runs on a built in kind, so
        // each one is waiting on a restart and none on a plugin.
        assert!(pending.iter().any(|p| p.contains("cam-wide")), "{pending:?}");
        assert!(pending.iter().all(|p| !p.contains("waits for")), "{pending:?}");
        assert!(pending.iter().any(|p| p.contains("restart")), "{pending:?}");
        // The camera plugin is still named, on the plan rather than here.
        assert!(plan.missing().iter().any(|p| p.name == "camera"));
    }
}
