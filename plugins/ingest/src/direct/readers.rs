//! Which shows the station reads through the relay: those with an HLS
//! output, which the station packages from the show's stream (or the
//! output's rendition) on the hub. The relay's loopback port is opened for
//! them when no channel has opened it already.

use super::host::Host;

impl Host {
    pub fn readers(&self) -> Vec<String> {
        let reads = |s: &&super::show::Show| s.row.outputs.iter().any(|w| w.platform == "hls");
        self.lock().values().filter(reads).map(|s| s.row.id.clone()).collect()
    }
}
