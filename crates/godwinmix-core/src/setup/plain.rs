//! The refusals a person reads when a piece is missing.
//!
//! Each is an `Actionable`: one or two plain sentences, the button that moves
//! things on, and the developer's detail in `data.detail`. The tests in
//! `tests.rs` hold every sentence here to the rule that it names no program,
//! setting, file or plugin id.

use super::names::{noun, sentence_start, WEB};
use super::web::Lookup;
use godwinmix_protocol::{Actionable, ErrorAction};
use serde_json::json;

/// What a person reads while a piece is being set up for them.
pub fn setting_up(piece: &str) -> String {
    let how_long = if piece == WEB {
        "a few minutes"
    } else if crate::plugin::first_party::ships_built(piece) {
        // The installer carried it built: a copy, not a build.
        "a moment"
    } else {
        "about a minute"
    };
    format!(
        "Setting up {}. This happens once and takes {how_long}; it starts by itself when it is ready.",
        noun(piece)
    )
}

/// A web page asked for while the renderer is being set up, or before the
/// set up has been asked for. The button starts it, or waits on it.
pub fn web_setting_up(lookup: &Lookup) -> Actionable {
    Actionable::new(setting_up(WEB), ErrorAction::setup("Set up web pages", WEB)).with_detail(lookup.detail())
}

/// No renderer, and nothing here can make one. A package that left it out
/// is the only way to get here, so the sentence says to get the full one.
pub fn web_unavailable(lookup: &Lookup) -> Actionable {
    let mut detail = lookup.detail();
    if cfg!(target_os = "linux") {
        detail["lighter_renderer"] = json!("the gstreamer1.0-wpe package (Debian and Ubuntu) gives wpesrc, which the mixer uses instead");
    }
    Actionable::new(
        "Web pages are not part of this copy of the mixer. Install the full download, which \
         includes them, or open Settings to point the mixer at a browser renderer you already have.",
        ErrorAction::open_setting("Open Settings", "browser.sidecar"),
    )
    .with_detail(detail)
}

/// The operator pointed the mixer at a renderer that is not there.
pub fn web_configured_missing(lookup: &Lookup) -> Actionable {
    Actionable::new(
        "Web pages cannot start: the browser renderer chosen in Settings is not there any more. \
         Clear that setting and the mixer uses its own.",
        ErrorAction::open_setting("Open Settings", "browser.sidecar"),
    )
    .with_detail(lookup.detail())
}

/// A source whose kind comes from a plugin that is not installed.
///
/// A plugin that ships with the mixer is set up with one press, and nothing
/// goes off air. Anything else is installed by name, from a marketplace.
pub fn plugin_missing(name: &str, type_id: &str, shipped: bool) -> Actionable {
    let detail = json!({ "plugin": name, "type": type_id, "installed": false, "shipped": shipped });
    if shipped || super::names::known(name) {
        return Actionable::new(
            format!(
                "{} {} not set up on this mixer yet. Press Set up; it takes about a minute and \
                 nothing goes off air.",
                sentence_start(name),
                super::names::be(name)
            ),
            ErrorAction::setup(&format!("Set up {}", noun(name)), name),
        )
        .with_detail(detail);
    }
    Actionable::new(
        "This kind of source needs an add on this mixer does not have yet. Press Install to fetch it; \
         nothing goes off air.",
        ErrorAction { label: "Install".into(), ..ErrorAction::install_plugin(name) },
    )
    .with_detail(detail)
}

/// Installed and switched off.
pub fn plugin_off(name: &str) -> Actionable {
    let (what, be, them) = if super::names::known(name) {
        let be = super::names::be(name);
        (sentence_start(name), be, if be == "is" { "it" } else { "them" })
    } else {
        ("This kind of source".into(), "is", "it")
    };
    Actionable::new(
        format!("{what} {be} switched off on this mixer. Turn {them} back on; nothing goes off air."),
        ErrorAction { label: "Turn them on".into(), ..ErrorAction::enable_plugin(name) },
    )
    .with_detail(json!({ "plugin": name, "installed": true, "enabled": false }))
}

/// What a person is told for an error, and the detail kept for a developer.
///
/// An `Actionable` with a detail is a sentence written for a person, so it
/// is told alone; the engine's context lines around it ("adding source
/// lyrics", "building the pipeline") name ids and steps, and go to the
/// detail as `chain`. Anything else is the whole chain, as it always was.
pub fn for_person(err: &anyhow::Error) -> (String, Option<serde_json::Value>) {
    let full = format!("{err:#}");
    match Actionable::find(err.as_ref()) {
        Some(a) if a.detail.is_some() => {
            let mut detail = a.detail.clone().unwrap_or_default();
            if let Some(map) = detail.as_object_mut() {
                map.insert("chain".into(), serde_json::Value::String(full));
            }
            (a.message.clone(), Some(detail))
        }
        _ => (full, None),
    }
}
