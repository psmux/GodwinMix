//! MPEG-TS segments beside the CMAF ones, for players that take nothing
//! else: old set top boxes, and whatever read Flussonic's URLs.
//!
//! ```text
//!   hub frames ─► TsCut (main: the picture and the first sound) ─► TsTrack "main"
//!              ─► TsCut (a further sound alone)                ─► TsTrack "audio1", "audio2"
//! ```
//!
//! Made in Rust from the frames the packager already reads, never by a
//! GStreamer muxer. The muxer is a copy of the ingest plugin's `tsmux`
//! (that crate is a binary, so it cannot be linked), cut down to H.264,
//! HEVC and AAC; `es`, `packet`, `psi` and `clock` are its files as they
//! are there. The segments are cut where `cmafmux` cut the CMAF picture,
//! read off its ring, so both playlists number and time the same segments.

mod clock;
pub mod cut;
mod es;
pub mod listing;
pub mod mux;
mod packet;
mod psi;
pub mod ring;

pub use cut::TsCut;
pub use listing::{clock_name, parse_clock, Entry, Master, Media};
pub use mux::{Unit, UnitKind};
pub use ring::TsTrack;

/// Which segment a request names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum At {
    /// By its media sequence number.
    Msn(u64),
    /// By its start, in ms of wall clock: the station's own `{start_ms}.ts`.
    Start(i64),
    /// By the second it starts in and its length: Flussonic's names.
    Clock { unix_s: i64, duration_ms: u32 },
    /// By the second it starts in alone.
    Second(i64),
}

/// The rendition id a sound track's TS has: `audio1` for track 1. Track 0
/// rides in `main`.
pub fn sound_id(track: u8) -> String {
    if track == 0 { "audio".into() } else { format!("audio{track}") }
}

/// The TS rendition with the picture and the first sound.
pub const MAIN: &str = "main";
