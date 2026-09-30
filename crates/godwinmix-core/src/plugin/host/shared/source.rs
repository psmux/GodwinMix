//! `SharedSource`: the `Source` the mixer holds for a device that is shared.
//!
//! It always reads the bus. Whether this source is also the owner, running
//! the plugin that opens the device, is the owner thread's business and can
//! change while the source runs; the mixer never sees it change.

use super::feed::Plan;
use super::owner::Owner;
use super::reader::{self, Watch};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::plugin::kinds::BuildCtx;
use crate::plugin::source::Source;
use crate::plugin::{Capability, Configure, Health, Hello, Manifest, MediaEnds, PluginState, Ready};
use anyhow::{Context, Result};
use godwinmix_framebus::BusName;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

pub struct SharedSource {
    type_id: String,
    name: BusName,
    dir: PathBuf,
    manifest: Manifest,
    build: BuildCtx,
    watch: Arc<Watch>,
    owner: Option<Owner>,
}

impl SharedSource {
    pub fn make(type_id: String, name: BusName, build: BuildCtx) -> Result<Box<dyn Source>> {
        let manifest = crate::plugin::loader::source_provide(&type_id)
            .map(|p| p.manifest)
            .with_context(|| format!("`{type_id}` is not a loaded source provide"))?;
        Ok(Box::new(SharedSource {
            type_id,
            name,
            dir: super::bus_dir(),
            manifest,
            build,
            watch: Arc::default(),
            owner: None,
        }))
    }

    fn plan(&self) -> Plan {
        Plan {
            type_id: self.type_id.clone(),
            name: self.name.clone(),
            dir: self.dir.clone(),
            build: self.build.clone(),
        }
    }

    fn is_owner(&self) -> bool {
        self.owner.as_ref().is_some_and(Owner::is_owner)
    }

    /// What `call("share")` answers: the name, who owns it from here, and what
    /// the bus has cost this source.
    fn report(&self) -> Value {
        let shared = self.owner.as_ref().map(|o| o.shared.clone());
        let pid = shared.as_ref().and_then(|s| s.feed.lock().as_ref().and_then(|f| f.pid()));
        let publish = shared.as_ref().and_then(|s| s.feed.lock().as_ref().map(|f| f.through.report()));
        let mut out = json!({
            "bus": self.name.to_string(),
            "dir": self.dir.display().to_string(),
            "owner": self.is_owner(),
            "plugin_pid": pid,
            "publish_ms": publish,
            "takeovers": shared.as_ref().map_or(0, |s| s.takeovers.load(Relaxed)),
            "last_start_ms": shared.as_ref().map_or(0, |s| s.last_start_ms.load(Relaxed)),
        });
        if let (Some(obj), Value::Object(read)) = (out.as_object_mut(), self.watch.report()) {
            obj.extend(read);
        }
        out
    }

    /// Run `f` on the plugin process, which only the owner has.
    fn on_plugin<R>(&self, f: impl FnOnce(&mut super::super::SidecarSource) -> Result<R>) -> Result<R> {
        let not_here = || {
            anyhow::anyhow!(
                "{} is opened by another source, which is where its plugin runs. The change \
                 reaches the picture only from there; `call share` on this source says which",
                self.name
            )
        };
        let owner = self.owner.as_ref().ok_or_else(not_here)?;
        let mut feed = owner.shared.feed.lock();
        let feed = feed.as_mut().ok_or_else(not_here)?;
        f(feed.sidecar())
    }
}

impl Source for SharedSource {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.build.canvas = hello.canvas;
        self.build.cfg.params = hello.params;
        // `gmxbussrc` stamps each frame from the clock and base time the mixer
        // gives this pipeline, which is what `programme-timeline` means,
        // whatever the plugin itself declares.
        let mut capabilities = self.manifest.capabilities;
        capabilities.set(Capability::ProgrammeTimeline, true);
        Ok(Ready { manifest: self.manifest, latency_ms: self.manifest.latency_ms, capabilities })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.build.canvas = canvas.clone();
        let ends = reader::build(&self.build, thumb, &self.name, &self.dir, &self.watch)?;
        if self.owner.is_none() {
            let owner = Owner::spawn(self.plan(), self.watch.clone());
            self.owner = Some(owner.context("starting the share thread")?);
        }
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        if self.watch.frames() > 0 {
            tracing::info!(source = %self.build.id, share = %self.report(), "stopping a shared source");
        }
        // Dropping the owner stops its thread and, if this source owned the
        // device, the plugin with it; the claim goes last, and a reader
        // elsewhere takes it within one tick.
        self.owner = None;
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        match super::plan(&self.type_id, params) {
            Some(name) if name == self.name => {}
            _ => {
                return Ok(Configure::RestartRequired(format!(
                    "the change names a different device than {}",
                    self.name
                )))
            }
        }
        self.build.cfg.params = params.clone();
        if let Some(owner) = &self.owner {
            if let Some(plan) = owner.shared.plan.lock().as_mut() {
                plan.build.cfg.params = params.clone();
            }
        }
        if !self.is_owner() {
            // The owner's settings decide the picture. Saying applied is true
            // for this source: it will use them if it becomes the owner.
            return Ok(Configure::Applied);
        }
        self.on_plugin(|p| p.configure(params))
    }

    fn health(&self) -> Health {
        if self.is_owner() {
            if let Ok(h) = self.on_plugin(|p| Ok(p.health())) {
                return h;
            }
        }
        let quiet = self.watch.quiet_ms();
        Health {
            state: match quiet {
                Some(ms) if ms < 2000 => PluginState::Running,
                Some(_) => PluginState::Stalled,
                None => PluginState::Starting,
            },
            detail: Some(format!("reads {} from the frame bus", self.name)),
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            "share" => Ok(self.report()),
            "restart" if !self.is_owner() => Ok(json!({"respawned": false, "owner": false})),
            _ => self.on_plugin(|p| p.call(method, params)),
        }
    }
}
