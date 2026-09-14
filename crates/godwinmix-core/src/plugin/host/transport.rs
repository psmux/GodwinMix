//! The media end of a sidecar, per transport.
//!
//! Three ways a plugin's picture reaches the core, negotiated at the handshake
//! from the manifest's list, in the core's order of preference:
//!
//! | Transport | Element here | Cost | Where |
//! |---|---|---|---|
//! | `unixfd` | `unixfdsrc` on a socket the core names | zero copy | Linux, macOS |
//! | `shm` | `shmsrc` on a socket the core names | one copy a frame | Linux, macOS |
//! | `container` | `fdsrc` (or the Windows reader thread's `appsrc`) into `decodebin` | a demux, and a decode if it was encoded | everywhere |
//!
//! The container path is exactly what `exec/source` does today, which is the
//! point: the seed of the plugin protocol was already the right shape, and a
//! sidecar is that shape with a handshake in front of it.
//!
//! The two socket transports carry one stream per socket, so a plugin with
//! both video and audio is given a base address and uses `<base>.video` and
//! `<base>.audio`. That is written down in `docs/reference/plugin-lifecycle.md`
//! and is what the SDK opens.

use crate::caps::CanvasCaps;
use crate::gstutil::make;
use anyhow::{Context, Result};
use godwinmix_protocol::plugin::wire::Transport;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::path::{Path, PathBuf};

/// Where a socket transport's addresses live while an instance runs.
///
/// One directory per instance under the runtime directory, removed when the
/// instance goes, so `plugin.add` then `plugin.remove` leaves no sockets
/// behind. The leak counting test checks exactly that.
pub struct MediaDir {
    path: PathBuf,
}

impl MediaDir {
    /// Make the directory for one instance.
    pub fn create(runtime: &Path, instance: &str) -> Result<Self> {
        let path = runtime.join("plugins").join(instance);
        std::fs::create_dir_all(&path)
            .with_context(|| format!("making the media directory {}", path.display()))?;
        Ok(Self { path })
    }

    /// The base address handed to the plugin in `GMX_MEDIA` and in the
    /// handshake answer. The plugin appends `.video` and `.audio`.
    pub fn base(&self) -> String {
        self.path.join("media").to_string_lossy().into_owned()
    }

    pub fn video(&self) -> String {
        format!("{}.video", self.base())
    }

    pub fn audio(&self) -> String {
        format!("{}.audio", self.base())
    }

    /// The FIFO an output plugin reads the programme from. See `output.rs`
    /// for why an output's media does not travel on stdin.
    pub fn programme(&self) -> String {
        format!("{}.programme", self.base())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for MediaDir {
    fn drop(&mut self) {
        // Every socket in it went with the process. Removing the directory is
        // what makes the leak count come back to zero.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Can this build carry that transport, with the elements it actually has?
///
/// `Transport::available_here` answers for the platform; this answers for the
/// GStreamer installation, which is the one that catches a Linux box built
/// without `gst-plugins-bad`.
pub fn usable(transport: Transport) -> Result<()> {
    match transport {
        Transport::Container => Ok(()),
        Transport::Unixfd => {
            anyhow::ensure!(
                cfg!(unix),
                "the `unixfd` transport needs Unix sockets and this is {}. The plugin's \
                 manifest should list 'container', which works everywhere.",
                std::env::consts::OS
            );
            anyhow::ensure!(
                crate::probe::exists("unixfdsrc"),
                "the `unixfd` transport needs the GStreamer `unixfdsrc` element, which this \
                 installation does not have (it arrived in GStreamer 1.24, in \
                 gst-plugins-bad). Install it, or have the plugin declare 'container'."
            );
            Ok(())
        }
        Transport::Shm => {
            anyhow::ensure!(
                cfg!(unix),
                "the `shm` transport needs Unix sockets and this is {}. The plugin's manifest \
                 should list 'container', which works everywhere.",
                std::env::consts::OS
            );
            anyhow::ensure!(
                crate::probe::exists("shmsrc"),
                "the `shm` transport needs the GStreamer `shmsrc` element, which this \
                 installation does not have (gst-plugins-bad). Install it, or have the plugin \
                 declare 'container'."
            );
            Ok(())
        }
    }
}

/// The transports this build can actually carry, for an error message and for
/// `plugin.describe`.
pub fn available() -> Vec<Transport> {
    Transport::ORDER.into_iter().filter(|t| usable(*t).is_ok()).collect()
}

/// A raw video source on a socket, already capped at the canvas contract.
///
/// The caps are set here rather than trusted from the plugin: a source that
/// says it is sending I420 at the canvas size and sends something else is
/// caught at the capsfilter, one element after it arrived, instead of three
/// elements later inside the compositor.
pub fn socket_video(
    transport: Transport,
    id: &str,
    address: &str,
    canvas: &CanvasCaps,
) -> Result<Vec<gst::Element>> {
    let (factory, name) = element_for(transport);
    let src = make(factory, &format!("{id}-src-{name}"))?;
    src.set_property("socket-path", address);
    // A source that is not there yet is waited for rather than refused: the
    // core creates the address and the plugin binds it, and which of the two
    // is ready first is a race nothing should depend on.
    crate::probe::set_bool(&src, "is-live", true);
    let filter = make("capsfilter", &format!("{id}-caps-{name}-video"))?;
    filter.set_property("caps", canvas.video());
    Ok(vec![src, filter])
}

/// The same for audio.
pub fn socket_audio(
    transport: Transport,
    id: &str,
    address: &str,
    canvas: &CanvasCaps,
) -> Result<Vec<gst::Element>> {
    let (factory, name) = element_for(transport);
    let src = make(factory, &format!("{id}-src-{name}-audio"))?;
    src.set_property("socket-path", address);
    crate::probe::set_bool(&src, "is-live", true);
    let filter = make("capsfilter", &format!("{id}-caps-{name}-audio"))?;
    filter.set_property("caps", canvas.audio());
    Ok(vec![src, filter])
}

fn element_for(transport: Transport) -> (&'static str, &'static str) {
    match transport {
        Transport::Unixfd => ("unixfdsrc", "unixfd"),
        Transport::Shm => ("shmsrc", "shm"),
        Transport::Container => ("fdsrc", "container"),
    }
}

/// A decoder for whatever container the plugin writes.
///
/// `decodebin` does the demuxing and the decoding and respects the ranks the
/// hardware probe set, so a container plugin is accelerated on a GPU box and
/// falls back to software on one without, exactly like every other source.
pub fn decoder(id: &str) -> Result<gst::Element> {
    make("decodebin", &format!("{id}-decode"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_container_is_always_usable_and_is_in_the_list() {
        usable(Transport::Container).expect("a pipe works everywhere");
        assert!(available().contains(&Transport::Container));
    }

    #[cfg(not(unix))]
    #[test]
    fn windows_refuses_the_socket_transports_and_names_the_container() {
        for t in [Transport::Unixfd, Transport::Shm] {
            let err = usable(t).expect_err("not on this platform");
            assert!(format!("{err}").contains("container"), "{err}");
        }
    }

    #[test]
    fn a_media_directory_is_named_per_instance_and_removed_with_it() {
        let runtime = std::env::temp_dir().join(format!("gmx-media-{}", std::process::id()));
        let path = {
            let dir = MediaDir::create(&runtime, "cam1").expect("the directory is made");
            assert!(dir.path().exists());
            assert!(dir.video().ends_with(".video"));
            assert!(dir.audio().ends_with(".audio"));
            assert!(dir.base().contains("cam1"));
            dir.path().to_path_buf()
        };
        assert!(!path.exists(), "dropping the instance takes its sockets with it");
        let _ = std::fs::remove_dir_all(&runtime);
    }
}
