//! Which config keys `config.set` may change, and when each one takes effect.
//!
//! Type, description and default come from the structs themselves, through
//! `schemars` (see `schema.rs`). What cannot be derived is here: a short title
//! for a form, the range a value has to fall in, and the honest answer to
//! "does the running mixer take this now". That answer was worked out by
//! reading where each value is held:
//!
//! * `live`: read from the mixer's own config every time it is used (the stall
//!   policy each tick, the audio ramp on each take, the exec gate on each
//!   `source.add`), or held behind a lock that `config.set` moves (`[safety]`).
//! * `next_source`: read when a source is built, so a source already running
//!   keeps what it was built with (`[browser]`).
//! * `restart`: copied into something built at start. The encoder, the
//!   multiview, the snapshot tracker, the media library, the listening
//!   socket, the token table and the plugin host all hold their own copy.
//!
//! A test holds this table and the derived schema to the same set of keys, so
//! a field added to a section without a row here fails rather than drifting.

use serde::Serialize;

use super::Config;

/// When a change to a key takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Applies {
    /// In force as soon as `config.set` answers.
    Live,
    /// Used by every source added or rebuilt after the change.
    NextSource,
    /// Written to the file and used from the next start.
    Restart,
}

impl Applies {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::NextSource => "next_source",
            Self::Restart => "restart",
        }
    }
}

/// One settable key.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    /// Dotted, the same spelling as `ConfigChange.key` in a preset plan.
    pub key: &'static str,
    pub title: &'static str,
    pub applies: Applies,
    /// Never read back: `config.get` says whether one is set, and nothing more.
    pub secret: bool,
    pub min: Option<i64>,
    pub max: Option<i64>,
    /// The words a string may be, when it is one of a few.
    pub choices: &'static [&'static str],
    pub unit: Option<&'static str>,
}

impl Key {
    const fn new(key: &'static str, title: &'static str, applies: Applies) -> Self {
        Self { key, title, applies, secret: false, min: None, max: None, choices: &[], unit: None }
    }
    const fn range(mut self, min: i64, max: i64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }
    const fn unit(mut self, unit: &'static str) -> Self {
        self.unit = Some(unit);
        self
    }
    const fn choices(mut self, choices: &'static [&'static str]) -> Self {
        self.choices = choices;
        self
    }
    const fn secret(mut self) -> Self {
        self.secret = true;
        self
    }
    /// The section a form groups this key under: `canvas`, `program`.
    pub fn section(&self) -> &'static str {
        self.key.split('.').next().unwrap_or(self.key)
    }
}

use Applies::{Live, NextSource, Restart};

const ACCEL: &[&str] = &[
    "auto", "nvidia", "va", "qsv", "amf", "videotoolbox", "mediafoundation", "d3d11", "d3d12",
    "cuda", "gl", "vulkan", "v4l2", "software",
];

/// Every key `config.set` takes, in the order a form shows them.
pub const KEYS: &[Key] = &[
    Key::new("canvas.width", "Picture width", Restart).range(16, 7680).unit("px"),
    Key::new("canvas.height", "Picture height", Restart).range(16, 4320).unit("px"),
    Key::new("canvas.fps", "Frame rate", Restart).range(1, 120).unit("fps"),
    Key::new("canvas.sample_rate", "Audio sample rate", Restart).range(8000, 192000).unit("Hz"),
    Key::new("canvas.channels", "Audio channels", Restart).range(1, 2),
    Key::new("program.video_bitrate_kbps", "Video bitrate", Restart).range(100, 100_000).unit("kbit/s"),
    Key::new("program.audio_bitrate_kbps", "Audio bitrate", Restart).range(32, 512).unit("kbit/s"),
    Key::new("program.keyframe_interval_secs", "Keyframe interval", Restart).range(1, 10).unit("s"),
    Key::new("program.audio_ramp_ms", "Audio ramp on a take", Live).range(0, 5000).unit("ms"),
    Key::new("program.av_offset_ms", "Hold video behind audio", Restart).range(-2000, 2000).unit("ms"),
    Key::new("program.encoder", "When the encoder runs", Restart).choices(&["on-demand", "always"]),
    Key::new("multiview.enabled", "Multiview", Restart),
    Key::new("multiview.width", "Multiview width", Restart).range(16, 3840).unit("px"),
    Key::new("multiview.height", "Multiview height", Restart).range(16, 2160).unit("px"),
    Key::new("multiview.fps", "Multiview frame rate", Restart).range(1, 60).unit("fps"),
    Key::new("multiview.jpeg_quality", "Multiview picture quality", Restart).range(1, 100),
    Key::new("multiview.include_program", "Show the programme in the multiview", Restart),
    Key::new("multiview.linger_secs", "Keep the multiview up after the last viewer", Restart).range(0, 3600).unit("s"),
    Key::new("snapshot.enabled", "Snapshots", Restart),
    Key::new("snapshot.default_width", "Snapshot width", Restart).range(16, 3840).unit("px"),
    Key::new("snapshot.min_interval_secs", "Least time between snapshots", Restart).range(0, 3600).unit("s"),
    Key::new("snapshot.max_width", "Largest snapshot", Restart).range(16, 7680).unit("px"),
    Key::new("snapshot.idle_secs", "Stop tracking motion after", Restart).range(0, 3600).unit("s"),
    Key::new("control.bind", "Control address", Restart),
    Key::new("control.ui_dir", "Web UI folder", Restart),
    Key::new("control.plugins_dir", "Plugins folder", Restart),
    Key::new("control.token", "Control token", Restart).secret(),
    Key::new("hardware.decode", "Hardware decoding", Restart).choices(ACCEL),
    Key::new("hardware.encode", "Hardware encoding", Restart).choices(ACCEL),
    Key::new("hardware.graphics", "Hardware compositing", Restart).choices(ACCEL),
    Key::new("media.dir", "Media folder", Restart),
    Key::new("media.max_depth", "Folder depth to list", Restart).range(0, 32),
    Key::new("media.max_files", "Most files to list", Restart).range(1, 100_000),
    Key::new("media.probe_timeout_secs", "Time to read one file", Restart).range(1, 60).unit("s"),
    Key::new("media.allow_upload", "Allow uploads", Restart),
    Key::new("media.max_upload_bytes", "Largest upload", Restart).range(1, i64::MAX).unit("bytes"),
    Key::new("media.convert_threads", "Threads for a conversion", Restart).range(1, 64),
    Key::new("security.allow_exec_sources", "Allow exec sources", Live),
    Key::new("safety.min_hold_ms", "Least time between takes", Live).range(0, 600_000).unit("ms"),
    Key::new("safety.max_takes_per_minute", "Most takes a minute", Live).range(1, 6000),
    Key::new("safety.flash_guard", "Flash guard", Live),
    Key::new("safety.on_operator_silence.after_secs", "Operator silence after", Live).range(1, 86_400).unit("s"),
    Key::new("safety.on_operator_silence.action", "When the operator goes quiet", Live),
    Key::new("browser.sidecar", "Browser sidecar", NextSource),
    Key::new("browser.args", "Browser arguments", NextSource),
    Key::new("browser.env", "Browser environment", NextSource),
    Key::new("browser.overlay_fps", "Page overlay frame rate", NextSource).range(1, 120).unit("fps"),
    Key::new("stall.restart_after_secs", "Rebuild a silent source after", Live).range(1, 3600).unit("s"),
    Key::new("stall.rebuild_attempts", "Quick rebuilds before backing off", Live).range(0, 100),
    Key::new("stall.rebuild_backoff_secs", "First backoff", Live).range(0, 3600).unit("s"),
    Key::new("stall.rebuild_backoff_max_secs", "Longest backoff", Live).range(0, 86_400).unit("s"),
    Key::new("stall.hold_last_frame", "Hold the last frame while rebuilding", Live),
    Key::new("nodes.listen", "Node bridge address", Restart),
    Key::new("nodes.clock_port", "Clock port", Restart).range(1, 65535),
    Key::new("nodes.clock", "Clock", Restart).choices(&["net", "ptp"]),
    Key::new("nodes.server_names", "Names this mixer is reached by", Restart),
    Key::new("nodes.advertise", "Advertise on the network", Restart),
    Key::new("plugins.allow_unsigned", "Allow unsigned plugins", Restart),
    Key::new("plugins.allow_wasi", "Plugins given WASI access", Restart),
];

/// The row for a key, if `config.set` takes it.
pub fn find(key: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.key == key)
}

/// For a key `config.set` does not take, the method that owns it, so the
/// refusal can say where to go instead.
pub fn owner_of(key: &str) -> Option<&'static str> {
    let section = key.split('.').next().unwrap_or(key);
    Some(match section {
        "sources" => "source.add and source.set",
        "outputs" => "output.add and output.set",
        "filters" => "filter.add and filter.set",
        "plugins" => "plugin.settings.set",
        "ui" => "preset.apply",
        "tokens" => "the [[tokens]] table in the config file; no method writes it yet",
        "codecs" => "the [codecs] table in the config file; no method writes it yet",
        _ => return None,
    })
}

/// Copy what a running mixer takes without a restart from `from` into `into`.
///
/// Every `live` and `next_source` key that the mixer loop reads from its own
/// config. `[safety]` is copied too although the guard holds its own, because
/// the guard is moved separately by `Guard::set_config` and this keeps the two
/// the same.
pub fn take_live(into: &mut Config, from: &Config) {
    into.program.audio_ramp_ms = from.program.audio_ramp_ms;
    into.security.allow_exec_sources = from.security.allow_exec_sources;
    into.safety = from.safety.clone();
    into.browser = from.browser.clone();
    into.stall = from.stall.clone();
}
