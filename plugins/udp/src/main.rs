//! `udp/source` and `udp/output`: MPEG-TS over UDP, unicast and multicast.
//!
//! This is how broadcast plant moves video around a building: an IRD, an
//! encoder or a playout server puts MPEG-TS on a multicast group, and anything
//! on the network that wants it joins the group. The source receives such a
//! feed (bare TS or RTP wrapped, one program out of a multiplex), and the
//! output puts the programme on one.
//!
//! One process serves one provide. `GMX_PROVIDE` says which.

mod output;
mod source;

// The rest lives in the library (src/lib.rs), so the direct host can share it.
use gmx_udp::{recv, send};

use godwinmix_sdk::prelude::*;

fn main() {
    let env = PluginEnv::from_env();
    if !env.started_by_core() {
        eprintln!(
            "gmx-udp is a GodwinMix plugin: the core starts it and talks JSON lines on stdin \
             and stderr.\nInstall it with `gmx plugin add plugins/udp`, then add a source of \
             type udp/source or an output of type udp/output.\nSee plugins/udp/README.md."
        );
        std::process::exit(2);
    }
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gmx-udp could not read its own gmx-plugin.toml: {e}");
            std::process::exit(1);
        }
    };
    let outcome = match env.provide.as_str() {
        "output" => runtime::run(&manifest, OutputHandler(output::UdpOutput::new())),
        _ => runtime::run(&manifest, SourceHandler(source::UdpSource::new())),
    };
    if let Err(e) = outcome {
        eprintln!("udp/{} stopped: {e}", env.provide);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use godwinmix_sdk::prelude::*;

    #[test]
    fn the_shipped_manifest_passes_the_validator_the_harness_runs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let manifest = Manifest::load(root.join("gmx-plugin.toml")).expect("the manifest must validate");
        assert_eq!(manifest.plugin.name, "udp");
        let ids: Vec<&str> = manifest.provides.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["source", "output"]);
    }
}
