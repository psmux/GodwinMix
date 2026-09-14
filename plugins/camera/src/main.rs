//! A USB or built in camera as a GodwinMix source.
//!
//! One process per instance, started by the core. Control is JSON-RPC 2.0 on
//! stdin and stderr, one object per line; the SDK runs that loop. Media leaves
//! over whichever transport the handshake chose: raw I420 on a Unix socket
//! where there is one, and a streamable Matroska stream on stdout everywhere
//! else, Windows included.
//!
//! The manifest declares two provides and `GMX_PROVIDE` says which one this
//! process is:
//!
//! | Provide | What it is |
//! |---|---|
//! | `source` | one camera, one instance |
//! | `devices` | a singleton that answers `discover` with every camera |
//!
//! There is no audio here on purpose. A camera with a microphone in it appears
//! as a separate audio device, and using it means a second source of type
//! `audio-device/source`. That is what lets an operator take the picture from
//! a camera and the sound from a desk, which is what every church does.

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
    // Say hello before opening anything. A camera that takes a second to wake
    // must not eat the five seconds the handshake is allowed.
    let outcome = match env.provide.as_str() {
        "devices" => runtime::run(&manifest, DeviceHandler(discover::CameraDevices::new())),
        _ => runtime::run(&manifest, SourceHandler(source::CameraSource::new())),
    };
    if let Err(e) = outcome {
        eprintln!("gmx-camera stopped: {e}");
        std::process::exit(1);
    }
}
