//! What a `SharedSource` asks of its owner thread: the plan a feed starts
//! from, whether this source runs the plugin, and the numbers it reports.

use super::feed::Plan;
use super::owner::Owner;
use super::reader;
use super::source::SharedSource;
use anyhow::Result;
use serde_json::{json, Value};
use std::sync::atomic::Ordering::Relaxed;

impl SharedSource {
    pub(super) fn plan(&self) -> Plan {
        Plan {
            type_id: self.type_id.clone(),
            name: self.name.clone(),
            dir: self.dir.clone(),
            build: self.build.clone(),
            tracks: self.tracks(),
        }
    }

    pub(super) fn tracks(&self) -> reader::Tracks {
        let media = self.manifest.media;
        reader::Tracks { video: media.video.present(), audio: media.audio.present() }
    }

    pub(super) fn is_owner(&self) -> bool {
        self.owner.as_ref().is_some_and(Owner::is_owner)
    }

    /// What `call("share")` answers: the name, who owns it from here, and what
    /// the bus has cost this source.
    pub(super) fn report(&self) -> Value {
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
    pub(super) fn on_plugin<R>(&self, f: impl FnOnce(&mut super::super::SidecarSource) -> Result<R>) -> Result<R> {
        let not_here = || {
            anyhow::anyhow!(
                "{} is opened by another source, in this mixer or in another show, and that \
                 source's settings decide the picture. Change it there, or stop that source \
                 and this one takes the device over within a second",
                self.name
            )
        };
        let owner = self.owner.as_ref().ok_or_else(not_here)?;
        let mut feed = owner.shared.feed.lock();
        let feed = feed.as_mut().ok_or_else(not_here)?;
        f(feed.sidecar())
    }
}
