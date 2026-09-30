//! `rtsp/output`: the programme served over RTSP.
//!
//! RTSP is what a hardware decoder, a network video recorder, a surveillance
//! wall, VLC or another mixer uses to pull a stream from a box on the
//! network. This serves the programme at `rtsp://<this machine>:<port>/<path>`
//! on a port the person chose, open for as long as the output exists. Nothing
//! is encoded again: the programme's encode is packetised as it is.

mod feed;
mod ingest;
mod output;
mod server;
mod settings;

#[cfg(test)]
mod tests;

use godwinmix_sdk::prelude::*;

fn main() {
    let env = PluginEnv::from_env();
    if !env.started_by_core() {
        eprintln!(
            "gmx-rtsp is a GodwinMix plugin: the core starts it and talks JSON lines on stdin \
             and stderr.\nInstall it with `gmx plugin add plugins/rtsp`, then add an output of \
             type rtsp/output.\nSee plugins/rtsp/README.md."
        );
        std::process::exit(2);
    }
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gmx-rtsp could not read its own gmx-plugin.toml: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = runtime::run(&manifest, OutputHandler(output::RtspOutput::new())) {
        eprintln!("rtsp/output stopped: {e}");
        std::process::exit(1);
    }
}
