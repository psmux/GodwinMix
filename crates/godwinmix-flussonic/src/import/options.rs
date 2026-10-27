//! How a config is read into shows: what its `deviceid` does.

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

/// How a config is read into shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub deviceid: DeviceId,
}
