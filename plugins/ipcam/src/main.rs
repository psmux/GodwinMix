//! `ipcam/source` and `ipcam/discover`: IP cameras.
//!
//! The source takes what a camera serves over HTTP: an MJPEG stream, or a
//! snapshot picture asked for a few times a second. Discovery finds ONVIF
//! cameras on the LAN and hands back each profile's RTSP address, which the
//! core then opens as a stream. One process serves one provide;
//! `GMX_PROVIDE` says which.

mod camera;
mod discover;
mod fetch;
mod onvif;
mod settings;
mod snapshot;
mod source;

#[cfg(test)]
mod tests;

use godwinmix_sdk::prelude::*;

fn main() {
    let env = PluginEnv::from_env();
    if !env.started_by_core() {
        eprintln!(
            "gmx-ipcam is a GodwinMix plugin: the core starts it and talks JSON lines on stdin and \
             stderr.\nInstall it with `gmx plugin add plugins/ipcam`, then add a source of type \
             ipcam/source, or find cameras with device discovery.\nSee plugins/ipcam/README.md."
        );
        std::process::exit(2);
    }
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gmx-ipcam could not read its own gmx-plugin.toml: {e}");
            std::process::exit(1);
        }
    };
    let outcome = match env.provide.as_str() {
        "discover" => runtime::run(&manifest, DeviceHandler(discover::Discover::default())),
        _ => runtime::run(&manifest, SourceHandler(source::IpCam::default())),
    };
    if let Err(e) = outcome {
        eprintln!("ipcam/{} stopped: {e}", env.provide);
        std::process::exit(1);
    }
}
