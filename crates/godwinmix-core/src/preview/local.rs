//! Raw frames to a client on the same machine, over a Unix socket, with no
//! encode at all.
//!
//! # Who this is for
//!
//! A native client on the same host as the core: a Rust or Python tool, a C#
//! designer, a Godot scene. MJPEG costs an encode per frame and a decode at the
//! other end; WHEP costs a video encoder and a whole ICE negotiation. Neither is
//! worth paying when the two processes share a page cache. `unixfdsink` passes
//! the buffer's file descriptor across, so the frame is never copied and never
//! compressed.
//!
//! This is the same transport the media contract in 03 uses between the core
//! and a sidecar plugin, pointed at a preview client instead.
//!
//! # Platforms
//!
//! Linux and macOS. Windows has no `unixfdsink`, and `preview.open` there
//! answers with the reason and the next step rather than a 500. Everything in
//! this file that touches a socket path is behind a `cfg`, and the Windows half
//! is a plain function that returns the refusal, so both halves compile
//! everywhere.

use crate::state::SourceId;

/// What `preview.open` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Program,
    Source(SourceId),
}

impl Target {
    pub fn parse(target: &str) -> Self {
        match target {
            "program" | "programme" => Self::Program,
            id => Self::Source(id.to_string()),
        }
    }

    /// The file name this target's socket takes. A slug, never a UUID, so a
    /// person reading `ls` knows what they are looking at.
    pub fn slug(&self) -> String {
        match self {
            Self::Program => "program".into(),
            Self::Source(id) => id.clone(),
        }
    }
}

/// Whether this platform could serve a local raw preview at all.
///
/// A `cfg` and nothing else, so it can be asked before GStreamer is
/// initialised. `godwinmix --info` runs on a machine that may have no
/// GStreamer at all, exactly as `--api-info` does, and must not ask the
/// registry anything.
pub fn supported_platform() -> bool {
    cfg!(unix)
}

/// Whether this core can serve one right now: the platform, and the element.
///
/// Needs an initialised GStreamer, so it is asked from the mixer and from a
/// request, never from an early exit flag.
pub fn supported() -> bool {
    supported_platform() && crate::probe::exists("unixfdsink")
}

/// Why it cannot, when it cannot. The message names the state and the next
/// step, as every refusal in this codebase does.
pub fn unsupported_message() -> String {
    if !cfg!(unix) {
        return "a local raw preview needs a Unix socket and this core is running on Windows. \
                Use /mjpeg/program, which costs one JPEG encode per frame and works everywhere, \
                or /whep/program for raw speed with audio."
            .to_string();
    }
    "a local raw preview needs the GStreamer element 'unixfdsink', which is not installed. \
     It is in gst-plugins-base 1.24 and later. Install it and restart the core, or use \
     /mjpeg/program until then."
        .to_string()
}

/// Where a target's socket lives under `runtime_dir`.
///
/// Printed by `godwinmix --info` and returned by `preview.open`, so a client
/// never has to guess or construct it.
pub fn socket_path(runtime_dir: &std::path::Path, target: &Target) -> std::path::PathBuf {
    runtime_dir.join("preview").join(format!("{}.sock", target.slug()))
}

/// The directory the sockets live in, created on demand.
pub fn socket_dir(runtime_dir: &std::path::Path) -> std::path::PathBuf {
    runtime_dir.join("preview")
}

#[cfg(unix)]
mod imp {
    use super::*;
    use crate::gstutil::{self, make};
    use anyhow::{Context, Result};
    use gstreamer as gst;
    use gstreamer::prelude::*;
    use tracing::{debug, warn};

    /// A `unixfdsink` on a preview branch, and the socket it listens on.
    ///
    /// Dropping it unlinks the branch, takes it to NULL and removes the socket
    /// file, so a core that has stopped leaves nothing in the runtime directory
    /// for the next one to trip over.
    pub struct LocalPreview {
        target: Target,
        path: std::path::PathBuf,
        tee: gst::Element,
        pad: Option<gst::Pad>,
        branch: Vec<gst::Element>,
        pipeline: gst::Pipeline,
    }

    impl LocalPreview {
        /// Hang a `unixfdsink` off a raw video tee.
        pub fn build(
            pipeline: &gst::Pipeline,
            tee: &gst::Element,
            target: Target,
            path: std::path::PathBuf,
        ) -> Result<Self> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("making {}", dir.display()))?;
            }
            // A socket left by a core that did not shut down cleanly would stop
            // this one binding.
            let _ = std::fs::remove_file(&path);

            let tag = target.slug();
            let queue = gstutil::queue_preview(&format!("uf-q-{tag}"))?;
            let sink = make("unixfdsink", &format!("uf-sink-{tag}"))?;
            sink.set_property("socket-path", path.to_string_lossy().to_string());
            // A preview client that goes away must not become an error on a tee
            // the programme also hangs off.
            crate::probe::set_bool(&sink, "sync", false);

            let branch = vec![queue, sink];
            pipeline.add_many(&branch).context("adding a local preview branch")?;
            gst::Element::link_many(branch.iter().collect::<Vec<_>>())
                .context("linking a local preview branch")?;

            let mut preview =
                Self { target, path, tee: tee.clone(), pad: None, branch, pipeline: pipeline.clone() };
            preview.attach()?;
            debug!(path = %preview.path.display(), "local raw preview open");
            Ok(preview)
        }

        fn attach(&mut self) -> Result<()> {
            let head = self.branch.first().context("an empty local preview branch")?;
            let sink = head.static_pad("sink").context("the branch head has no sink pad")?;
            let pad = self
                .tee
                .request_pad_simple("src_%u")
                .context("the raw video tee refused a pad for a local preview")?;
            pad.link(&sink).context("linking a local preview onto the raw tee")?;
            for el in self.branch.iter().rev() {
                el.sync_state_with_parent().ok();
            }
            self.pad = Some(pad);
            Ok(())
        }

        pub fn path(&self) -> &std::path::Path {
            &self.path
        }

        pub fn target(&self) -> &Target {
            &self.target
        }
    }

    impl Drop for LocalPreview {
        fn drop(&mut self) {
            if let Some(pad) = self.pad.take() {
                if let Some(peer) = pad.peer() {
                    if let Err(e) = pad.unlink(&peer) {
                        warn!(?e, "could not unlink a local preview branch");
                    }
                }
                self.tee.release_request_pad(&pad);
            }
            for el in self.branch.iter().rev() {
                // Locked first, so the bin's own state walk cannot put it
                // back to PLAYING between NULL and the remove and dispose it
                // running. See `Encoder::detach`.
                el.set_locked_state(true);
                let _ = el.set_state(gst::State::Null);
                let _ = self.pipeline.remove(el);
            }
            let _ = std::fs::remove_file(&self.path);
            debug!(path = %self.path.display(), "local raw preview closed");
        }
    }
}

#[cfg(unix)]
pub use imp::LocalPreview;

/// On Windows there is no socket to open, so there is no type to hold one.
/// Callers ask [`supported`] first and report [`unsupported_message`].
#[cfg(not(unix))]
pub struct LocalPreview;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_reads_the_names_a_client_types_and_slugs_them() {
        assert_eq!(Target::parse("program"), Target::Program);
        assert_eq!(Target::parse("programme"), Target::Program);
        assert_eq!(Target::parse("cam1"), Target::Source("cam1".into()));
        assert_eq!(Target::parse("cam1").slug(), "cam1");
        assert_eq!(Target::Program.slug(), "program");
    }

    #[test]
    fn a_socket_path_is_readable_and_lives_under_the_runtime_directory() {
        let dir = std::path::Path::new("/run/gmx");
        let p = socket_path(dir, &Target::Source("cam1".into()));
        assert_eq!(p, std::path::Path::new("/run/gmx/preview/cam1.sock"));
        assert!(p.starts_with(socket_dir(dir)));
    }

    #[test]
    fn the_platform_check_asks_gstreamer_nothing() {
        // The point of it: `--info` runs before gst::init and must not panic.
        assert_eq!(supported_platform(), cfg!(unix));
    }

    #[test]
    fn the_refusal_names_the_state_and_the_next_step() {
        let m = unsupported_message();
        assert!(m.contains("/mjpeg/program"), "{m}");
        if cfg!(unix) {
            assert!(m.contains("unixfdsink"), "{m}");
        } else {
            assert!(m.contains("Windows"), "{m}");
        }
    }
}
