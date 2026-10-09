//! What the station and its HLS packager say to each other, over the
//! packager's loopback port.
//!
//! | Request | Body | Answer |
//! |---|---|---|
//! | `PUT /packager/outputs` | every [`Want`] | 204 once applied |
//! | `GET /packager/outputs` | | every [`Report`] |
//! | `GET /hls/...` | a player's request, let in by the station | the playlist or the segment |
//!
//! Every request carries `Authorization: Bearer <secret>`, the secret the
//! station gave the process in [`SECRET_ENV`] when it started it, and a
//! request for `/hls` names its show in [`SHOW_HEADER`] and the player's
//! address in [`PEER_HEADER`], which the viewer count is kept by.

use godwinmix_core::hls::HlsParams;
use godwinmix_protocol::destination::DestinationLive;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// The secret every request to the packager must carry.
pub const SECRET_ENV: &str = "GODWINMIX_PACKAGER_SECRET";
/// The show a forwarded `/hls` request is for.
pub const SHOW_HEADER: &str = "x-godwinmix-show";
/// The address the player reached the station from.
pub const PEER_HEADER: &str = "x-godwinmix-peer";
/// What the packager writes on its stdout once it listens, before the
/// address.
pub const LISTENING: &str = "GODWINMIX-PACKAGER";
/// The flag that starts this binary as the packager.
pub const FLAG: &str = "--hls-packager";
/// What a channel's watch link has in place of a show, in every key and
/// header: `channel:sunday-service`. A show id never has a colon in it.
pub const CHANNEL: &str = "channel:";
/// Where the station hands over the outputs and reads them back.
pub const OUTPUTS: &str = "/packager/outputs";

/// What one packager reads off the ingest plugin's relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub relay: SocketAddr,
    /// `direct.<show>/main`, or a rendition's `direct.<show>/main|<video>|<audio>`.
    pub path: String,
}

/// One HLS output that should run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Want {
    pub show: String,
    pub output: String,
    /// None while there is nothing to read yet, and then `why_not` says why.
    pub source: Option<Source>,
    pub why_not: Option<String>,
    pub segment_ms: u32,
    pub part_ms: u32,
    pub window_s: u32,
    pub viewer_key: String,
}

impl Want {
    pub fn params(&self) -> HlsParams {
        HlsParams { segment_ms: self.segment_ms, part_ms: self.part_ms, window_s: self.window_s }
    }
}

/// What one output is doing, as the packager sees it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    pub show: String,
    pub output: String,
    pub live: DestinationLive,
    pub viewers: u32,
}
