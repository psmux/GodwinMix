//! A microphone, a line input or a sound card as a GodwinMix source.
//!
//! Sound only: F32LE, 48 kHz, stereo, ten milliseconds a buffer, which is the
//! media contract in `docs/reference/plugin-lifecycle.md`. The meters an
//! operator sees are the core's, measured where the sound reaches the mix, so
//! nothing here computes a level.
//!
//! `GMX_PROVIDE` says which of the manifest's two provides this process is:
//! `source` is one input, `devices` is the singleton that answers `discover`.

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
        "devices" => runtime::run(&manifest, DeviceHandler(discover::AudioDevices::new())),
        _ => runtime::run(&manifest, SourceHandler(source::AudioSource::new())),
    };
    if let Err(e) = outcome {
        eprintln!("gmx-audio-device stopped: {e}");
        std::process::exit(1);
    }
}
