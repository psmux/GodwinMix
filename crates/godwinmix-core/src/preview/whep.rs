//! WHEP: WebRTC out of the mixer, for anything that wants audio and under half
//! a second of delay.
//!
//! # What this needs and what it says when it is not there
//!
//! `whepserversink` lives in the Rust `webrtc` plugin (`gst-plugins-rs`), which
//! GStreamer 1.28 ships but which many distributions package separately. The
//! element is looked for once at startup rather than per request, and when it
//! is missing every `/whep/*` route answers 501 with the package to install,
//! naming the platform's own spelling. An operator should never have to read a
//! GStreamer error to find out that a plugin is not installed.
//!
//! # ICE and TURN
//!
//! A WebRTC connection needs a path between the browser and the core. On a LAN
//! the host candidates both ends already have are enough and nothing else is
//! needed. Across the internet, or out of a container with its own network
//! namespace, they are not, and a STUN server is what discovers the public
//! address. Where both ends are behind symmetric NAT even that fails and the
//! media has to be relayed by a TURN server.
//!
//! ```toml
//! [whep]
//! # Discover the public address. The default is Google's public STUN server,
//! # which is fine for trying it out and is somebody else's service to lose.
//! stun = "stun://stun.l.google.com:19302"
//! # Relay when a direct path cannot be found. Nothing is relayed unless it has
//! # to be, so a TURN server costs bandwidth only for the connections that need
//! # it. The password is in the URL, which is what libnice wants.
//! turn = ["turn://user:password@turn.example.com:3478"]
//! ```
//!
//! With `stun` set to an empty string nothing but host candidates is offered,
//! which is the right setting for a mixer on a LAN that should not be talking
//! to the internet at all.

use crate::probe;

/// The element every `/whep/*` route needs.
pub const ELEMENT: &str = "whepserversink";

/// Whether this build of GStreamer has the WHEP sink, asked once.
///
/// `probe::exists` walks the registry, which is cheap but not free, and every
/// WHEP route asks. Once at startup is enough: a plugin installed while the
/// mixer runs is not picked up by a running process anyway.
pub fn available() -> bool {
    static PRESENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PRESENT.get_or_init(|| probe::exists(ELEMENT))
}

/// What to tell a client on a build without the element.
///
/// Names the package for the platform the core is running on, because "install
/// gst-plugins-rs" is not something anybody can act on directly.
pub fn missing_message() -> String {
    format!(
        "WHEP is not available on this core: the GStreamer element '{ELEMENT}' is not \
         installed. It is in the Rust webrtc plugin. Install {}, restart the core, and \
         POST here again. Until then use /mjpeg/program for picture and /pcm/program or \
         /opus/program for sound, which need nothing extra.",
        package()
    )
}

/// The package that carries the Rust webrtc plugin on this platform.
fn package() -> &'static str {
    if cfg!(target_os = "macos") {
        "the GStreamer 1.28 runtime from gstreamer.freedesktop.org, or `brew install gst-plugins-rs`"
    } else if cfg!(target_os = "windows") {
        "the GStreamer 1.28 MSI from gstreamer.freedesktop.org, which carries it"
    } else {
        "`gstreamer1.0-plugins-rs` (Debian and Ubuntu) or `gstreamer1-plugins-rs` (Fedora)"
    }
}

/// What a `/whep/*` stream is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Program,
    Source(String),
}

impl Target {
    pub fn parse(segment: &str) -> Self {
        match segment {
            "program" | "programme" => Self::Program,
            id => Self::Source(id.to_string()),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Program => "program".into(),
            Self::Source(id) => id.clone(),
        }
    }
}

/// `[whep]`, the ICE configuration this module documents above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceConfig {
    /// A STUN URI, or empty for host candidates only.
    pub stun: String,
    /// TURN URIs, tried in order when no direct path is found.
    pub turn: Vec<String>,
}

impl Default for IceConfig {
    fn default() -> Self {
        Self { stun: "stun://stun.l.google.com:19302".into(), turn: Vec::new() }
    }
}

impl IceConfig {
    /// Put the ICE settings on a `whepserversink`.
    ///
    /// Properties are set through `probe`, which leaves an unknown one alone,
    /// so a build of the plugin that spells one of these differently degrades
    /// to host candidates rather than failing to start.
    pub fn apply(&self, sink: &gstreamer::Element) {
        if !self.stun.is_empty() {
            probe::set_str(sink, "stun-server", &self.stun);
        }
        if !self.turn.is_empty() {
            probe::set_str(sink, "turn-servers", &self.turn.join(","));
        }
    }

    /// Whether this core will talk to anything outside its own network to set
    /// a connection up. Worth saying in the docs and in `core.info`.
    pub fn is_lan_only(&self) -> bool {
        self.stun.is_empty() && self.turn.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refusal_names_the_element_the_package_and_the_next_step() {
        let m = missing_message();
        assert!(m.contains(ELEMENT), "{m}");
        assert!(m.contains("restart the core"), "{m}");
        // And it offers what does work today rather than leaving a dead end.
        assert!(m.contains("/mjpeg/program"), "{m}");
        assert!(m.contains("/pcm/program"), "{m}");
    }

    #[test]
    fn a_target_reads_both_spellings_of_programme() {
        assert_eq!(Target::parse("program"), Target::Program);
        assert_eq!(Target::parse("programme"), Target::Program);
        assert_eq!(Target::parse("cam1"), Target::Source("cam1".into()));
        assert_eq!(Target::parse("cam1").label(), "cam1");
    }

    #[test]
    fn an_empty_stun_means_host_candidates_only() {
        assert!(!IceConfig::default().is_lan_only());
        let lan = IceConfig { stun: String::new(), turn: vec![] };
        assert!(lan.is_lan_only());
        let relayed = IceConfig { stun: String::new(), turn: vec!["turn://x".into()] };
        assert!(!relayed.is_lan_only());
    }
}
