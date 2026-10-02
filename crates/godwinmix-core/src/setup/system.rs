//! What only the operating system can supply: a part of GStreamer.
//!
//! The mixer cannot build GStreamer's plugins for itself, so the refusal
//! names this platform's one command to install the missing part and the
//! page shows it with a copy button. The element names stay in the detail.

use godwinmix_protocol::{Actionable, ErrorAction};
use serde_json::json;

/// One part of GStreamer, by what it is called on each platform.
#[derive(Debug, Clone, Copy)]
pub struct Part {
    /// Homebrew formula names.
    pub brew: &'static str,
    /// Debian and Ubuntu package names.
    pub apt: &'static str,
    /// Fedora package names.
    pub dnf: &'static str,
}

/// libnice's elements, which every WebRTC session needs.
pub const NICE: Part = Part { brew: "libnice-gstreamer", apt: "gstreamer1.0-nice", dnf: "libnice-gstreamer1" };
/// The "bad" set: webrtcbin, srt, rist, rsvg and more.
pub const BAD: Part = Part { brew: "gstreamer", apt: "gstreamer1.0-plugins-bad", dnf: "gstreamer1-plugins-bad-free" };
/// The Rust set: cmafmux, whip, ndi.
pub const RS: Part = Part { brew: "gstreamer", apt: "gstreamer1.0-plugins-rs", dnf: "gstreamer1-plugins-rs" };
/// Everything a WebRTC session needs: webrtcbin and libnice's elements.
pub const WEBRTC: Part = Part {
    brew: "gstreamer libnice-gstreamer",
    apt: "gstreamer1.0-plugins-bad gstreamer1.0-nice",
    dnf: "gstreamer1-plugins-bad-free libnice-gstreamer1",
};
/// The OpenGL elements the lighter web page renderer draws through.
pub const GL: Part = Part { brew: "gstreamer", apt: "gstreamer1.0-gl", dnf: "gstreamer1-plugins-base" };
/// The "good" set: decoders, muxers, capture on Linux.
pub const GOOD: Part = Part { brew: "gstreamer", apt: "gstreamer1.0-plugins-good", dnf: "gstreamer1-plugins-good" };

/// The one command that installs `part` on this machine.
pub fn install_command(part: Part) -> String {
    if cfg!(target_os = "macos") {
        format!("brew install {}", part.brew)
    } else if cfg!(target_os = "windows") {
        // The official installer carries every set; there is no per set
        // package to name.
        "winget install --id gstreamerproject.gstreamer".to_string()
    } else if std::path::Path::new("/usr/bin/dnf").exists() {
        format!("sudo dnf install {}", part.dnf)
    } else {
        format!("sudo apt install {}", part.apt)
    }
}

/// `what` needs a part of GStreamer this machine does not have.
///
/// `what` is the person's words for the thing that does not work, such as
/// "Sending the programme to a browser". The command is the action, so the
/// page draws it with a copy button.
pub fn missing(what: &str, part: Part, elements: &[&str]) -> Actionable {
    let command = install_command(part);
    Actionable::new(
        format!(
            "{what} needs a part of GStreamer this machine does not have. Install it with the \
             command below, then restart the mixer."
        ),
        ErrorAction::copy("Copy the command", &command),
    )
    .with_detail(json!({ "elements": elements, "command": command, "package": part.apt }))
}

/// The same, as an error.
pub fn missing_error(what: &str, part: Part, elements: &[&str]) -> anyhow::Error {
    anyhow::Error::new(missing(what, part, elements))
}

/// The elements of `names` this GStreamer does not have.
pub fn absent(names: &[&'static str]) -> Vec<&'static str> {
    names.iter().copied().filter(|n| !crate::probe::exists(n)).collect()
}
