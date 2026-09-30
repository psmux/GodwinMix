//! `decklink/source` and `decklink/devices`: SDI and HDMI capture through a
//! Blackmagic DeckLink card, with GStreamer's `decklink` elements.
//!
//! Built and tested for everything but the card itself: no machine this was
//! written on has one. Every step that needs the card says which of the
//! element, the driver or the card is missing, and discovery lists nothing
//! (rather than failing) on a machine without one.

mod card;
mod handlers;
mod settings;

use godwinmix_sdk::prelude::*;

fn main() {
    let env = PluginEnv::from_env();
    if !env.started_by_core() {
        eprintln!(
            "gmx-decklink is a GodwinMix plugin: the core starts it and talks JSON lines on stdin and \
             stderr.\nInstall it with `gmx plugin add plugins/decklink`.\nSee plugins/decklink/README.md."
        );
        std::process::exit(2);
    }
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gmx-decklink could not read its own gmx-plugin.toml: {e}");
            std::process::exit(1);
        }
    };
    let outcome = match env.provide.as_str() {
        "devices" => runtime::run(&manifest, DeviceHandler(handlers::Devices)),
        _ => runtime::run(&manifest, SourceHandler(handlers::DeckLink::default())),
    };
    if let Err(e) = outcome {
        eprintln!("decklink/{} stopped: {e}", env.provide);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests;
