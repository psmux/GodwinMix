//! `mode = "rpc"`: the plugin gets a JSON-RPC call on its stdin channel.
//!
//! The mode a plugin author reaches for, because the plugin is already a
//! process that speaks the protocol and a hook is one more method on it. The
//! call is `hook`, with `{hook, ts, payload}` as its params, and the answer is
//! read for `take.before` and thrown away for everything else.
//!
//! ## The gap this fills, and the one it leaves
//!
//! The core has no registry of running plugin *instances* that anything can
//! call: `plugin/loader.rs` interns source provides and remembers pids, and
//! only a source is ever instantiated (`intern_all` skips every other kind).
//! `SidecarService` exists and nothing constructs it. So there was no instance
//! to dispatch a hook to.
//!
//! What happens here instead: the first time an `rpc` hook fires for a plugin,
//! that plugin is started once as a sidecar, handshaken, and kept. Every later
//! hook is a call on the same process. It is started for its `service`
//! provide where it has one, and for its first provide otherwise, which is the
//! "dispatch to any plugin instance the loader can reach" of the brief.
//!
//! Two consequences worth knowing, and they are in `docs/reference/hooks.md`
//! as well as here:
//!
//! * The hook process is a second instance of the plugin, not the one already
//!   running as a source. A plugin whose hook needs to see its own source's
//!   state has to keep that state somewhere both can read, or use `http`.
//! * `process = "shared"` in the manifest is not honoured yet, for the same
//!   reason: there is no shared instance to join.
//!
//! When the host grows a real service instantiation, this file becomes a
//! lookup in it and the rest of the hook path does not change.

use anyhow::Context;
use godwinmix_core::hooks::{blocks, Decision, Hook};
use godwinmix_core::plugin::host::Sidecar;
use godwinmix_core::plugin::loader;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The wire method a hook arrives as. A plugin implements it the way it
/// implements `health`.
pub const METHOD: &str = "hook";

/// One long lived process per plugin that asked for an `rpc` hook.
#[derive(Default)]
pub struct Sidecars {
    /// An async mutex, because starting a plugin waits on a handshake and
    /// holding a blocking lock across that would stall the runtime.
    live: tokio::sync::Mutex<BTreeMap<String, Arc<Mutex<Sidecar>>>>,
}

impl Sidecars {
    /// Call one hook on a plugin, starting the plugin if this is the first.
    pub async fn call(&self, plugin: &str, hook: &Hook, body: Value) -> anyhow::Result<Decision> {
        let child = self.get_or_start(plugin).await?;
        let within = hook.timeout;
        let event = hook.event.clone();
        // The channel is blocking (a pipe and a condvar), so the call goes on
        // a blocking worker. The Tokio thread is free the whole time, which is
        // what lets `take.before` time out cleanly and abandon this.
        let answer = tokio::task::spawn_blocking(move || {
            child.lock().call_within(METHOD, body, within)
        })
        .await
        .context("the hook call was dropped")?;
        match answer {
            Ok(value) if blocks(&event) => Ok(Decision::parse(&value)),
            Ok(_) => Ok(Decision::Allow),
            Err(e) => Err(e),
        }
    }

    /// Stop and forget one plugin's hook process. `plugin.remove` calls this.
    pub fn drop_plugin(&self, plugin: &str) {
        let Ok(mut live) = self.live.try_lock() else { return };
        if let Some(child) = live.remove(plugin) {
            child.lock().shutdown("the plugin was removed");
        }
    }

    async fn get_or_start(&self, plugin: &str) -> anyhow::Result<Arc<Mutex<Sidecar>>> {
        let mut live = self.live.lock().await;
        if let Some(child) = live.get(plugin) {
            // A process that died between hooks is started again rather than
            // called into: the supervisor's job, done here in one line because
            // a hook process has no media and nothing to rebuild.
            if child.lock().running() {
                return Ok(child.clone());
            }
            live.remove(plugin);
        }
        let started = start(plugin)?;
        let child = Arc::new(Mutex::new(started));
        live.insert(plugin.to_string(), child.clone());
        Ok(child)
    }
}

/// Which provide to start a plugin's hook process from.
///
/// A `service` provide is what 03 section 2 says a hook belongs to. Anything
/// else is a fallback, and the reason is the gap in the module docs.
fn hook_provide(installed: &loader::Installed) -> anyhow::Result<String> {
    let provides = &installed.manifest.provides;
    let decl = provides
        .iter()
        .find(|p| p.kind == "service")
        .or_else(|| provides.first())
        .with_context(|| {
            format!(
                "the plugin `{}` asked for an rpc hook but provides nothing to run. Give it a \
                 `[[provides]] kind = \"service\"` block, or change the hook to mode = \
                 \"command\" or \"http\".",
                installed.name()
            )
        })?;
    Ok(format!("{}/{}", installed.name(), decl.id))
}

fn start(plugin: &str) -> anyhow::Result<Sidecar> {
    let installed = loader::get(plugin).with_context(|| {
        format!("no plugin called `{plugin}` is installed, so its hook cannot be called")
    })?;
    let type_id = hook_provide(&installed)?;
    let instance = format!("{plugin}-hooks");
    let token = loader::mint_token(&type_id, &instance);
    let launched = loader::launch_for(&type_id, &instance, token, loader::rpc_url())?;
    let mut child = Sidecar::spawn(&instance, &launched.launch)
        .with_context(|| format!("starting the hook process for `{plugin}`"))?;
    let canvas = godwinmix_core::plugin::host::source::canvas_of(
        &godwinmix_core::plugin::harness::test_canvas(),
    );
    child
        .handshake(Some(&launched.plugin), canvas, &launched.provide, Value::Object(Default::default()), |t| {
            // A hook process carries no media at all, so the address it is
            // handed is a name it will never open. Saying so plainly is
            // better than inventing a socket nothing listens on.
            Ok(format!("none:{}", t.as_str()))
        })
        .with_context(|| format!("the hook process for `{plugin}` did not say hello"))?;
    tracing::info!(plugin, instance, "started a hook process");
    Ok(child)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_plugin_that_is_not_installed_says_so_and_names_the_fix() {
        let sidecars = Sidecars::default();
        let hook = godwinmix_core::hooks::HookConfig {
            event: "take.before".into(),
            plugin: Some("nowhere".into()),
            ..godwinmix_core::hooks::HookConfig::default()
        }
        .build(0)
        .unwrap();
        let e = sidecars.call("nowhere", &hook, Value::Null).await.unwrap_err();
        assert!(format!("{e:#}").contains("no plugin called `nowhere`"), "{e:#}");
    }

    #[test]
    fn the_hook_process_prefers_a_service_provide() {
        use godwinmix_protocol::plugin::manifest::{Manifest, PluginMeta, Provide};
        let meta = PluginMeta {
            name: "compliance".into(),
            version: "1.0.0".into(),
            api: 1,
            description: String::new(),
            license: String::new(),
            authors: Vec::new(),
            repository: String::new(),
            platforms: Vec::new(),
            placements: vec!["sidecar".into()],
            process: "per-instance".into(),
        };
        let manifest = Manifest {
            plugin: meta,
            run: None,
            provides: vec![
                Provide { kind: "source".into(), id: "cam".into(), ..Provide::default() },
                Provide { kind: "service".into(), id: "policy".into(), ..Provide::default() },
            ],
            tools: Vec::new(),
            hooks: Default::default(),
            build: None,
        };
        let installed = loader::Installed {
            manifest,
            root: std::path::PathBuf::new(),
            enabled: true,
            provides: Vec::new(),
            tools: Vec::new(),
            hooks: Vec::new(),
            budget: Default::default(),
            problem: None,
        };
        assert_eq!(hook_provide(&installed).unwrap(), "compliance/policy");
    }
}
