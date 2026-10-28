//! How a config is read into shows: what its `deviceid` and its
//! `deinterlace=false` do.

use serde::Serialize;

/// What an imported `transcoder deviceid=N` does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceId {
    /// Every channel is placed on the GPU with the least work (`gpu:
    /// "auto"`). Flussonic writes `deviceid=0` on every stream unless told
    /// otherwise, so on a machine with two cards this is what splits the
    /// channels between them, and on a machine with one it is the same as
    /// the file. The default.
    #[default]
    Spread,
    /// `deviceid=N` puts the channel on GPU N, as written.
    File,
}

/// What an imported `transcoder deinterlace=false` does, which is what
/// Flussonic writes unless told otherwise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Deinterlacing {
    /// Deinterlace every picture the stream marks as interlaced
    /// (`deinterlace: "auto"`). Flussonic's `false` scaled both fields as
    /// one frame, which combs on every movement in a 1080i or 480i
    /// picture; a progressive one passes untouched, so this costs nothing
    /// where there is nothing to do. The default.
    #[default]
    Auto,
    /// `deinterlace=false` is off, as written.
    File,
}

/// How a config is read into shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub deviceid: DeviceId,
    pub deinterlace: Deinterlacing,
}
