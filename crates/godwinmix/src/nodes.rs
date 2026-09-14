//! Bringing the node bridge up beside the mixer, and keeping it honest.
//!
//! Three things happen here and nothing else: the machinery starts (only if
//! the config asked for it), what a node says is turned into the core's own
//! events and alerts, and the reconciler tick is carried out.
//!
//! None of it runs on a streaming thread, and none of it can stall the
//! encoder. The reconciler produces a list of decisions; this acts on them by
//! sending mixer commands, which are the same queue an operator's take goes
//! through.

use anyhow::{Context, Result};
use godwinmix_core::mixer::{self, MixerHandle};
use godwinmix_core::node;
use godwinmix_core::node::reconcile::Action;
use godwinmix_core::node::server::{NodeEvent, Watcher};
use godwinmix_core::plugin::host::LinkSource;
use godwinmix_core::state::Severity;
use std::sync::Arc;
use tracing::{info, warn};

/// Start the node bridge, if the config asked for one.
///
/// Returns the address it listened on, or `None` when nothing was asked for.
/// Nothing runs unless asked: a core with no `[nodes]` table makes no
/// certificate authority, opens no port and puts no clock on the network.
pub async fn start(
    cfg: &godwinmix_core::config::Config,
    handle: &MixerHandle,
    clock: Option<gstreamer::Clock>,
    canvas: &godwinmix_core::caps::CanvasCaps,
    runtime_dir: std::path::PathBuf,
) -> Result<Option<std::net::SocketAddr>> {
    if !cfg.nodes.wanted() {
        return Ok(None);
    }
    let clock = clock.context(
        "the programme pipeline has no clock yet, so nodes cannot be given one to follow",
    )?;
    let options = node::runtime::Options {
        dir: runtime_dir.join("nodes"),
        bind: cfg.nodes.bind(),
        server_names: cfg.nodes.names(),
        clock_port: cfg.nodes.clock_port,
        clock_kind: cfg.nodes.clock.clone(),
        advertise: cfg.nodes.advertise,
        clock,
        canvas: godwinmix_protocol::plugin::wire::Canvas {
            width: canvas.width as u32,
            height: canvas.height as u32,
            fps: (canvas.fps.numer() as u32) / (canvas.fps.denom().max(1) as u32),
        },
        expected: cfg
            .nodes
            .list
            .iter()
            .map(|(name, entry)| (name.clone(), entry.address.clone()))
            .collect(),
        watch: watcher(handle.clone()),
    };
    let bound = node::runtime::start(options).await?;
    info!(
        address = %bound,
        nodes = cfg.nodes.list.len(),
        "the node bridge is up; `gmx node token --name <node>` enrols a machine"
    );
    Ok(Some(bound))
}

/// Everything a node says, turned into what the rest of the core already
/// understands: an alert, an event, a log line, a loader state.
fn watcher(handle: MixerHandle) -> Watcher {
    Arc::new(move |event: NodeEvent| match event {
        NodeEvent::Joined { node, hello } => {
            info!(
                node = %node,
                version = %hello.version,
                platform = %hello.platform,
                plugins = hello.plugins.len(),
                "a node joined"
            );
            handle.publish_alert(
                Severity::Info,
                format!("the node {node} is online with {} plugins", hello.plugins.len()),
            );
        }
        NodeEvent::Left { node, why } => {
            warn!(node = %node, why = %why, "a node's bridge is down");
            // The alert an operator sees, and the freeze frame the viewer
            // sees, are two different mechanisms: the compositor is already
            // holding the last frame because no buffers are arriving. This
            // only says why.
            handle.publish_alert(
                Severity::Error,
                format!(
                    "the node {node} is unreachable ({why}). Its sources hold the freeze frame, \
                     then the slate; they come back on their own when it does"
                ),
            );
        }
        NodeEvent::Instance { node, instance, state, detail } => {
            godwinmix_core::plugin::loader::set_state(&instance, &state);
            tracing::debug!(node = %node, instance = %instance, state = %state, ?detail, "a remote instance");
        }
        NodeEvent::Event { node, instance, name, params } => {
            tracing::debug!(node = %node, ?instance, event = %name, ?params, "an event from a node");
        }
        NodeEvent::Log { node, instance, level, message } => {
            // A plugin's log line on a node lands in the core's log with its
            // instance tag, adjacent in time to the supervisor decision about
            // it, which is the whole point of 10 section 2.
            let instance = instance.unwrap_or_else(|| "-".into());
            match level.as_str() {
                "error" => tracing::error!(node = %node, instance = %instance, "{message}"),
                "warn" => warn!(node = %node, instance = %instance, "{message}"),
                "debug" | "trace" => tracing::debug!(node = %node, instance = %instance, "{message}"),
                _ => info!(node = %node, instance = %instance, "{message}"),
            }
        }
    })
}

/// Run the reconciler until the core stops.
///
/// One task, a quarter second apart. Every decision it makes turns into a
/// mixer command or an alert, and never into a wait.
pub fn spawn_reconciler(handle: MixerHandle, quit: Arc<tokio::sync::Notify>) {
    let Some(runtime) = node::runtime::get() else { return };
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(node::runtime::TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = quit.notified() => return,
                _ = tick.tick() => {}
            }
            let handle = handle.clone();
            runtime.tick(move |action| act(&handle, action));
        }
    });
}

/// Carry out one decision.
fn act(handle: &MixerHandle, action: &Action) {
    match action {
        // Starting and restarting are the same mixer command: rebuild that
        // source's pipeline. On a node that means the core asks it to spawn
        // the plugin again, which `BridgedSource::start` does.
        Action::Start { instance, .. } | Action::Restart { instance, .. } => {
            info!(why = %action.why(), "node reconciler");
            let _ = handle.send(mixer::Command::RestartSource(instance.clone()));
        }
        Action::Stop { instance, node, reason } => {
            info!(instance = %instance, node = %node, reason = %reason, "stopping a stray remote instance");
            if let Some(runtime) = node::runtime::get() {
                let _ = runtime.call(
                    node,
                    "node.stop",
                    serde_json::json!({ "instance": instance, "reason": reason }),
                );
            }
        }
        // Nothing to do: the backoff is doing it. Logged at debug by `tick`.
        Action::Wait { .. } => {}
        Action::Unreachable { instance, node, silent_ms } => {
            tracing::debug!(instance = %instance, node = %node, silent_ms, "a source is on a node that is not answering");
        }
        Action::Back { node } => {
            handle.publish_alert(
                Severity::Info,
                format!("the node {node} is answering again; its sources are being started back up"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handle with nothing on the other end. The two cases under test are
    /// both decided before anything is sent, so the queue is never read.
    fn detached_handle() -> MixerHandle {
        let cfg: godwinmix_core::config::Config = toml::from_str("").unwrap();
        gstreamer::init().unwrap();
        let (_mix, handle, _cmd_rx, _bus_rx) =
            godwinmix_core::mixer::Mixer::build(cfg).expect("build a mixer for the test");
        handle
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_core_with_no_nodes_table_starts_nothing() {
        let cfg: godwinmix_core::config::Config = toml::from_str("").unwrap();
        let canvas = godwinmix_core::caps::CanvasCaps::new(&cfg.canvas);
        let bound =
            start(&cfg, &detached_handle(), None, &canvas, std::env::temp_dir()).await.unwrap();
        assert!(bound.is_none(), "nothing runs unless asked");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_nodes_table_with_no_clock_says_so_rather_than_starting_half_of_it() {
        let cfg: godwinmix_core::config::Config =
            toml::from_str("[nodes]\nlisten = \"127.0.0.1:0\"\n").unwrap();
        let canvas = godwinmix_core::caps::CanvasCaps::new(&cfg.canvas);
        let e = start(&cfg, &detached_handle(), None, &canvas, std::env::temp_dir())
            .await
            .unwrap_err();
        assert!(e.to_string().contains("clock"), "{e}");
    }
}
