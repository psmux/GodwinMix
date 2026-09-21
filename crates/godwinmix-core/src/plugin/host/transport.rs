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

/// The longest base address a socket can be bound under.
///
/// A Unix socket's path is a fixed array in `sockaddr_un`: 104 bytes on macOS
/// and the BSDs, 108 on Linux, the terminator included. Past that the path is
/// cut, silently, and the socket is bound under whatever is left. The runtime
/// directory sits beside the config, so this is a real length: a desktop
/// install puts `macbook-pro-camera` at 101 bytes. One camera with a longer
/// name and its socket landed outside its own directory, where nothing removes
/// it, and every later start of that source failed with "Address already in
/// use", across restarts of the mixer too. Cut early enough, `.video` and
/// `.audio` become the same name.
///
/// `.programme` is the longest thing appended to a base, at ten bytes.
const LONGEST_BASE: usize = 104 - 1 - ".programme".len();

impl MediaDir {
    /// Make the directory for one instance.
    ///
    /// Under the runtime directory when the addresses fit there, which is
    /// where a person looks. Otherwise under the system's temporary directory
    /// with the instance's name hashed short, and refused with the numbers if
    /// even that is too long. Whatever a process that was killed left at the
    /// path goes first: an instance id belongs to one live instance, so nothing
    /// there is anybody's.
    pub fn create(runtime: &Path, instance: &str) -> Result<Self> {
        let path = Self::place(runtime, instance)?;
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .with_context(|| format!("making the media directory {}", path.display()))?;
        Ok(Self { path })
    }

    fn place(runtime: &Path, instance: &str) -> Result<PathBuf> {
        let fits = |dir: &Path| dir.join("media").as_os_str().len() <= LONGEST_BASE;
        let wanted = runtime.join("plugins").join(instance);
        if fits(&wanted) {
            return Ok(wanted);
        }
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (runtime, instance).hash(&mut hasher);
        let short = std::env::temp_dir()
            .join(format!("gmx-{}", std::process::id()))
            .join(format!("{:08x}", hasher.finish() as u32));
        anyhow::ensure!(
            fits(&short),
            "a media socket for {instance} needs an address of at most {LONGEST_BASE} bytes, and \
             neither {} nor {} is short enough. Move the config to a shorter path, or set TMPDIR \
             to one, or have the plugin declare the 'container' transport, which uses no socket.",
            wanted.display(),
            short.display()
        );
        Ok(short)
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
        // And the folder around it when this was the last one in it, which is
        // the per process one `place` makes under the temporary directory.
        // `remove_dir` refuses a directory that still has something in it.
        if let Some(parent) = self.path.parent() {
            if parent.parent() == Some(std::env::temp_dir().as_path()) {
                let _ = std::fs::remove_dir(parent);
            }
        }
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
        // Its own, so the test passes when it is the only one run.
        let _ = gst::init();
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

    /// A socket path is cut at 104 bytes on macOS with nothing said, and a cut
    /// path is a socket outside its directory that nothing ever removes.
    #[test]
    fn an_address_too_long_for_a_socket_is_moved_somewhere_short() {
        let deep = std::env::temp_dir().join("x".repeat(90)).join("runtime");
        let dir = MediaDir::create(&deep, "a-camera-with-quite-a-long-name").expect("placed");
        for address in [dir.video(), dir.audio(), dir.programme()] {
            assert!(address.len() < 104, "{} bytes: {address}", address.len());
        }
        assert!(!dir.path().starts_with(&deep), "it stayed where it cannot fit");
        let again = MediaDir::place(&deep, "a-camera-with-quite-a-long-name").unwrap();
        assert_eq!(again, dir.path(), "the same instance gets the same place");
        let other = MediaDir::place(&deep, "another-camera-with-a-long-name").unwrap();
        assert_ne!(other, dir.path(), "and another instance another");
    }

    /// A mixer that was killed leaves its sockets, and the next bind on one
    /// fails with "Address already in use".
    #[test]
    fn what_a_killed_process_left_is_cleared_before_the_next_one_binds() {
        let runtime = std::env::temp_dir().join(format!("gmx-stale-{}", std::process::id()));
        let first = MediaDir::create(&runtime, "cam1").unwrap();
        let stale = PathBuf::from(first.video());
        std::fs::write(&stale, b"left behind").unwrap();
        std::mem::forget(first);
        let second = MediaDir::create(&runtime, "cam1").unwrap();
        assert!(!stale.exists(), "the stale address is still there");
        drop(second);
        let _ = std::fs::remove_dir_all(&runtime);
    }
}
