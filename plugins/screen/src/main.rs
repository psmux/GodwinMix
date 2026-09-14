//! A monitor, or part of one, as a GodwinMix source.
//!
//! Desktop duplication: what is on the screen. Not a capture hook inside
//! another program, so an exclusive fullscreen game shows as black and the
//! answer is borderless windowed mode. That limit is deliberate. A hook is a
//! per game, per driver, per anti cheat problem that never stops being one,
//! and this plugin would spend its life chasing it.
//!
//! `GMX_PROVIDE` says which of the manifest's two provides this process is.

mod discover;
mod pipeline;
mod settings;
mod source;
mod tools;

use godwinmix_sdk::prelude::*;

fn main() {
    let env = PluginEnv::from_env();
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let outcome = match env.provide.as_str() {
        "devices" => runtime::run(&manifest, DeviceHandler(discover::ScreenDevices::new())),
        _ => runtime::run(&manifest, SourceHandler(source::ScreenSource::new())),
    };
    if let Err(e) = outcome {
        eprintln!("gmx-screen stopped: {e}");
        std::process::exit(1);
    }
}
