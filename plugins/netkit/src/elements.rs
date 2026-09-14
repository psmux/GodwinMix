//! Is the element here, and if not, where does it come from.
//!
//! A network plugin is mostly a wrapper around one or two GStreamer elements
//! that live outside the base set. When one is missing the plugin must say so
//! in a sentence the reader can act on, naming the package for the platform
//! they are on, rather than failing with "no element srtsrc".

/// Where an element comes from, per platform, for the error message.
pub struct Package {
    /// The element this describes, for example `srtsrc`.
    pub element: &'static str,
    /// Debian and Ubuntu package name.
    pub debian: &'static str,
    /// Homebrew formula or a phrase naming the bundle on macOS.
    pub macos: &'static str,
    /// What to install on Windows.
    pub windows: &'static str,
}

/// The packages the network plugins need, in one table so every message agrees.
pub const PACKAGES: &[Package] = &[
    Package {
        element: "srtsrc",
        debian: "gstreamer1.0-plugins-bad",
        macos: "brew install gstreamer (the bad set is in the same formula)",
        windows: "the GStreamer MSI, 'complete' install",
    },
    Package {
        element: "srtsink",
        debian: "gstreamer1.0-plugins-bad",
        macos: "brew install gstreamer (the bad set is in the same formula)",
        windows: "the GStreamer MSI, 'complete' install",
    },
    Package {
        element: "whipclientsink",
        debian: "gstreamer1.0-plugins-rs (GStreamer 1.28 or newer)",
        macos: "brew install gstreamer (1.28 or newer carries the rs webrtc set)",
        windows: "the GStreamer MSI 1.28 or newer, 'complete' install",
    },
    Package {
        element: "whipserversrc",
        debian: "gstreamer1.0-plugins-rs (GStreamer 1.28 or newer)",
        macos: "brew install gstreamer (1.28 or newer carries the rs webrtc set)",
        windows: "the GStreamer MSI 1.28 or newer, 'complete' install",
    },
    Package {
        element: "whepsrc",
        debian: "gstreamer1.0-plugins-rs (GStreamer 1.28 or newer)",
        macos: "brew install gstreamer (1.28 or newer carries the rs webrtc set)",
        windows: "the GStreamer MSI 1.28 or newer, 'complete' install",
    },
    Package {
        element: "ndisrc",
        debian: "gstreamer1.0-plugins-rs",
        macos: "brew install gstreamer (1.26 or newer carries the ndi set)",
        windows: "the GStreamer MSI 1.26 or newer, 'complete' install",
    },
    Package {
        element: "ndisink",
        debian: "gstreamer1.0-plugins-rs",
        macos: "brew install gstreamer (1.26 or newer carries the ndi set)",
        windows: "the GStreamer MSI 1.26 or newer, 'complete' install",
    },
    Package {
        element: "rtspclientsink",
        debian: "gstreamer1.0-rtsp",
        macos: "brew install gstreamer",
        windows: "the GStreamer MSI, 'complete' install",
    },
];

fn package_for(element: &str) -> Option<&'static Package> {
    PACKAGES.iter().find(|p| p.element == element)
}

/// Where this platform's copy of `element` comes from, as a phrase.
pub fn where_from(element: &str) -> String {
    let Some(p) = package_for(element) else {
        return "your GStreamer installation".into();
    };
    if cfg!(target_os = "linux") {
        format!("the '{}' package", p.debian)
    } else if cfg!(target_os = "macos") {
        p.macos.to_string()
    } else {
        p.windows.to_string()
    }
}

/// Is the element registered in this build of GStreamer?
///
/// [`crate::init`] must have run; a factory lookup before `gst_init` always
/// answers no.
pub fn exists(element: &str) -> bool {
    gstreamer::ElementFactory::find(element).is_some()
}

/// Every element in `wanted` that is not registered.
pub fn missing(wanted: &[&str]) -> Vec<String> {
    wanted
        .iter()
        .filter(|e| !exists(e))
        .map(|e| (*e).to_string())
        .collect()
}

/// `Ok(())` if every element is here, otherwise one sentence naming what is
/// missing and where it comes from.
pub fn require(wanted: &[&str]) -> Result<(), String> {
    let gone = missing(wanted);
    if gone.is_empty() {
        return Ok(());
    }
    let list = gone
        .iter()
        .map(|e| format!("'{e}' (from {})", where_from(e)))
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "this build of GStreamer has no {list}. Install it and run \
         `gst-inspect-1.0 {}` to check, then start the plugin again.",
        gone[0]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_package_row_names_all_three_platforms() {
        for p in PACKAGES {
            assert!(!p.debian.is_empty(), "{}", p.element);
            assert!(!p.macos.is_empty(), "{}", p.element);
            assert!(!p.windows.is_empty(), "{}", p.element);
        }
    }

    #[test]
    fn an_unknown_element_still_gets_a_phrase() {
        assert_eq!(where_from("nosuchelement"), "your GStreamer installation");
    }

    #[test]
    fn where_from_names_something_installable() {
        let phrase = where_from("srtsrc");
        assert!(!phrase.is_empty());
        assert!(phrase.len() > 4, "{phrase}");
    }

    #[test]
    fn require_of_a_nonsense_element_names_it_and_the_check_command() {
        crate::init().expect("gstreamer");
        let err = require(&["definitely-not-an-element"]).unwrap_err();
        assert!(err.contains("definitely-not-an-element"), "{err}");
        assert!(err.contains("gst-inspect-1.0"), "{err}");
    }

    #[test]
    fn require_of_a_base_element_passes() {
        crate::init().expect("gstreamer");
        require(&["fakesink", "fdsink"]).expect("fakesink and fdsink are in the base set");
    }
}
