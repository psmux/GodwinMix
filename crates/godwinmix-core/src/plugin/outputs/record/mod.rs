//! Portable recording behind the same output contract as network destinations.
mod files;
mod finalize;
pub use finalize::wait as wait_for_recordings;
mod pipeline;

use crate::config::Params;
use crate::plugin::output::{Output, OutputCtx, OutputProvide};
use crate::plugin::source::unknown_method;
use crate::plugin::{
    CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, PluginState, ProvideKind, Ready,
    StreamMode, Tier, API_LEVEL,
};
use anyhow::Result;
use gstreamer as gst;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

pub const MANIFEST: Manifest = Manifest {
    plugin: "record",
    id: "output",
    kind: ProvideKind::Output,
    api: API_LEVEL,
    description: "Record the encoded programme to a new MP4 or Matroska file",
    uri_schemes: &["record://"],
    rank: 250,
    media: MediaDecl {
        video: StreamMode::Container,
        audio: StreamMode::Container,
        alpha: false,
        thumb: false,
    },
    capabilities: CapabilitySet::new(),
    latency_ms: 0,
    tier: Tier::Core,
};
pub const PROVIDE: OutputProvide = OutputProvide {
    manifest: MANIFEST,
    claims: |uri| uri.starts_with("record://").then_some(250),
    make: |_| Ok(Box::new(Recording::default())),
};

#[derive(Default)]
struct Recording {
    folder: PathBuf,
    format: String,
    path: Option<PathBuf>,
    bytes: Arc<AtomicU64>,
}

impl Output for Recording {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        (self.folder, self.format) = files::settings(&hello.params)?;
        Ok(Ready {
            manifest: MANIFEST,
            latency_ms: 0,
            capabilities: MANIFEST.capabilities,
        })
    }

    fn build(
        &mut self,
        ctx: &OutputCtx<'_>,
        video: &gst::Element,
        audio: &gst::Element,
    ) -> Result<()> {
        let path = files::reserve(&self.folder, ctx.id, &self.format)?;
        self.bytes = Arc::new(AtomicU64::new(0));
        if let Err(error) = pipeline::build(
            ctx.pipeline,
            video,
            audio,
            &path,
            &self.format,
            self.bytes.clone(),
        ) {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        self.path = Some(path);
        Ok(())
    }

    fn connected(&self) -> bool {
        self.bytes.load(Ordering::Relaxed) > 0
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        files::settings(params)?;
        Ok(Configure::RestartRequired(
            "Stop this recording and start another to change its folder or format".into(),
        ))
    }

    fn health(&self) -> Health {
        Health::of(if self.connected() {
            PluginState::Running
        } else {
            PluginState::Starting
        })
    }

    fn call(&mut self, method: &str, _: Value) -> Result<Value> {
        match method {
            "stats" => Ok(json!(self.status())),
            _ => Err(unknown_method(&MANIFEST, method, &["stats"])),
        }
    }

    fn status(&self) -> godwinmix_protocol::types::Extra {
        let mut status = godwinmix_protocol::types::Extra::new();
        status.insert("type".into(), json!("record/output"));
        status.insert("recording_path".into(), json!(self.path));
        status.insert(
            "bytes_muxed".into(),
            json!(self.bytes.load(Ordering::Relaxed)),
        );
        status
    }

    fn shutdown(&mut self, pipeline: gst::Pipeline) {
        // Finalising a file can wait on the disk. It must never hold the mixer.
        finalize::finish(pipeline);
    }
}
