//! Record the programme to a file.
//!
//! An output plugin, which means the media travels the opposite way from a
//! source's: the core writes the encoded programme into a FIFO and this reads
//! it. Nothing here decodes anything, so a recording costs about as much CPU
//! as copying a file, whatever the canvas is.
//!
//! Two things are worth knowing before reading the code.
//!
//! The FIFO is opened at `initialize`, not at `start`. Opening a FIFO for
//! reading waits for a writer and opening it for writing waits for a reader,
//! and the core opens its end before it calls `start`; a plugin that waited
//! for `start` would deadlock with the core.
//!
//! MP4 is written in fragments. A recording of the thing you most wanted to
//! keep is usually the one the power cut interrupted, and an ordinary MP4 with
//! its index still in memory is a file no player will open.

mod naming;
mod output;
mod pipeline;
mod settings;
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
    if let Err(e) = runtime::run(&manifest, OutputHandler(output::FileRecorder::new())) {
        eprintln!("gmx-file-record stopped: {e}");
        std::process::exit(1);
    }
}
