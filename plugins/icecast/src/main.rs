//! `icecast/output` and `icecast/source`: audio over HTTP, the way internet
//! radio works.
//!
//! The output sends the programme's sound to an Icecast or SHOUTcast 2
//! server, which is how a church or a station reaches listeners who only want
//! to hear. The source plays an internet radio stream, or any audio stream
//! served over HTTP, as a live source. One process serves one provide;
//! `GMX_PROVIDE` says which.

mod handlers;
mod radio;
mod send;
mod settings;
mod station;

#[cfg(test)]
mod tests;

use godwinmix_sdk::prelude::*;

fn main() {
    let env = PluginEnv::from_env();
    if !env.started_by_core() {
        eprintln!(
            "gmx-icecast is a GodwinMix plugin: the core starts it and talks JSON lines on stdin and \
             stderr.\nInstall it with `gmx plugin add plugins/icecast`, then add an output of type \
             icecast/output or a source of type icecast/source.\nSee plugins/icecast/README.md."
        );
        std::process::exit(2);
    }
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gmx-icecast could not read its own gmx-plugin.toml: {e}");
            std::process::exit(1);
        }
    };
    let outcome = match env.provide.as_str() {
        "output" => runtime::run(&manifest, OutputHandler(handlers::IcecastOutput::default())),
        _ => runtime::run(&manifest, SourceHandler(station::RadioSource::default())),
    };
    if let Err(e) = outcome {
        eprintln!("icecast/{} stopped: {e}", env.provide);
        std::process::exit(1);
    }
}
