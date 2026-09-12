//! Configuration loaded from TOML at startup.
//!
//! The canvas section is the single most important part of this file: every
//! source, every ad and every slate is normalised to exactly these parameters
//! before it reaches a mixer. Changing them mid-broadcast is not possible, by
//! design, because the output encoder is never restarted.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub canvas: Canvas,
    #[serde(default)]
    pub program: ProgramConfig,
    #[serde(default)]
    pub multiview: MultiviewConfig,
    #[serde(default)]
    pub control: ControlConfig,
    #[serde(default)]
    pub hardware: HardwareConfig,
    #[serde(default)]
    pub media: MediaConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub browser: BrowserConfig,
    #[serde(default)]
    pub stall: StallConfig,
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    #[serde(default)]
    pub outputs: Vec<OutputConfig>,
}

/// The fixed raw format that every branch of the graph must produce.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub sample_rate: i32,
    pub channels: i32,
}

impl Default for Canvas {
    fn default() -> Self {
        Self { width: 1920, height: 1080, fps: 30, sample_rate: 48000, channels: 2 }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProgramConfig {
    /// Video bitrate in kbit/s for the outgoing program stream.
    pub video_bitrate_kbps: u32,
    /// Audio bitrate in kbit/s.
    pub audio_bitrate_kbps: u32,
    /// Keyframe interval in seconds. Two is what every CDN wants.
    pub keyframe_interval_secs: u32,
    /// Milliseconds to ramp audio across on a take. Zero gives you a click.
    pub audio_ramp_ms: u64,
    /// Milliseconds to hold video back relative to audio at the encoders.
    ///
    /// Unset means the known priming delay of the AAC encoder in use, which
    /// is what lands a page's beep on its flash to within a frame. Set it if
    /// a measurement on your output says otherwise (`browser/dev/measure-sync.py`).
    #[serde(default)]
    pub av_offset_ms: Option<i64>,
}

impl Default for ProgramConfig {
    fn default() -> Self {
        Self {
            video_bitrate_kbps: 6000,
            audio_bitrate_kbps: 160,
            keyframe_interval_secs: 2,
            audio_ramp_ms: 180,
            av_offset_ms: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MultiviewConfig {
    pub enabled: bool,
    /// Size of the whole mosaic, not of one cell.
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    /// JPEG quality, 1 to 100. The mosaic exists so an operator can tell what
    /// is in frame and pick a camera, not to judge picture quality, so this is
    /// deliberately modest. Bandwidth is roughly width * height * fps * q.
    pub jpeg_quality: u32,
    /// Include a program return cell in the grid.
    pub include_program: bool,
}

impl Default for MultiviewConfig {
    fn default() -> Self {
        // 960x540 at 8 fps and quality 60 costs roughly 2 Mbit/s, which a
        // remote operator on a hotel connection can actually receive.
        Self {
            enabled: true,
            width: 960,
            height: 540,
            fps: 8,
            jpeg_quality: 60,
            include_program: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ControlConfig {
    pub bind: String,
    /// Directory of static UI assets. Falls back to the embedded page.
    pub ui_dir: Option<String>,
    /// Bearer token every `/api/*` request and the WebSocket must carry.
    /// Unset means the control port is open to whoever can reach it, which is
    /// how it has always worked and is fine behind a firewall. The
    /// `LIVEBOXMIX_TOKEN` environment variable overrides this, so a deployment
    /// can keep the secret out of the config file. See `Config::token`.
    #[serde(default)]
    pub token: Option<String>,
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self { bind: "0.0.0.0:8080".into(), ui_dir: None, token: None }
    }
}

/// Hardware acceleration preference. `Auto` probes at startup and picks the
/// best available backend; the named variants force one and fail loudly if it
/// is not present, which is what you want on a server you control.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Accel {
    #[default]
    Auto,
    Nvidia,
    Va,
    VideoToolbox,
    MediaFoundation,
    D3d11,
    Software,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HardwareConfig {
    #[serde(default)]
    pub decode: Accel,
    #[serde(default)]
    pub encode: Accel,
}

/// Which RTMP client implementation to pull a source with.
///
/// This is not academic. GStreamer ships two, and they do not interoperate
/// with the same servers. `rtmp2src` is the modern one and works against
/// mediamtx, but against node-media-server it connects, is accepted, and then
/// silently delivers nothing at all: no error, no data. The older librtmp
/// based `rtmpsrc` works with both.
///
/// `Auto` starts with the modern client and falls back once if no media
/// arrives, so an unfamiliar server recovers on its own instead of leaving the
/// operator staring at a source stuck on "connecting".
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RtmpClient {
    #[default]
    Auto,
    /// `rtmp2src`, the modern implementation.
    Rtmp2,
    /// `rtmpsrc`, librtmp based. Widest server compatibility.
    Librtmp,
}

impl RtmpClient {
    pub const RTMP2_ELEMENT: &'static str = "rtmp2src";
    pub const LIBRTMP_ELEMENT: &'static str = "rtmpsrc";

    /// Element to try first.
    pub fn first_element(self) -> &'static str {
        match self {
            Self::Librtmp => Self::LIBRTMP_ELEMENT,
            Self::Auto | Self::Rtmp2 => Self::RTMP2_ELEMENT,
        }
    }

    /// Element to fall back to, if this setting permits a fallback at all.
    pub fn fallback_element(self) -> Option<&'static str> {
        matches!(self, Self::Auto).then_some(Self::LIBRTMP_ELEMENT)
    }
}

/// Things that widen what somebody reaching the control port can do.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SecurityConfig {
    /// Allow `exec:` sources, which run a command line on this machine.
    ///
    /// Off by default, and deliberately so. An `exec:` source is arbitrary code
    /// execution for anyone who can reach the control port, which is a much
    /// bigger grant than "can switch cameras". Turn it on only when the control
    /// port is on a trusted network.
    #[serde(default)]
    pub allow_exec_sources: bool,
}

/// How `web+` sources are rendered.
///
/// The preferred renderer is `liveboxmix-browser`, the CEF sidecar in
/// `browser/`: a full Chromium drawing off screen, handing over raw frames and
/// PCM with no encoder in between. When it is found, every `web+` source runs
/// through it. When it is not, `web+` falls back to GStreamer's `wpesrc`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BrowserConfig {
    /// Path to `liveboxmix-browser`. Unset means: look next to this executable,
    /// then on PATH. Set to a path that does not exist and `web+` sources fail
    /// with a clear message rather than silently falling back.
    #[serde(default)]
    pub sidecar: Option<String>,
    /// Extra arguments appended to the sidecar's command line.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment for the sidecar on top of the mixer's own. On a headless
    /// Linux box this is where `DISPLAY` and `PULSE_SINK` go.
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    /// Frames per second the browser draws at when it is only drawing the page
    /// over video the mixer decodes itself. See `default_overlay_fps`.
    #[serde(default = "default_overlay_fps")]
    pub overlay_fps: u32,
}

// Written out rather than derived. A derived Default would ignore the serde
// field defaults and give `overlay_fps` a zero, and a config file with no
// `[browser]` section takes exactly this path, so the sidecar would be told to
// draw at zero frames a second.
impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            sidecar: None,
            args: Vec::new(),
            env: std::collections::BTreeMap::new(),
            overlay_fps: default_overlay_fps(),
        }
    }
}

/// A superimposed page is chrome: a scoreboard, a lower third, a logo. It does
/// not need the canvas frame rate, and asking for it is expensive in a way that
/// is easy to miss. Those frames cross to the mixer raw, and raw frames with an
/// alpha channel are 4 bytes a pixel against I420's 1.5, so a 720p page at 30
/// fps is 107 MB/s down the pipe where an ordinary source is 41 MB/s. The
/// compositor holds the last page frame between updates, so the picture still
/// leaves at the canvas rate with the video moving at full speed underneath.
///
/// Ten is a compromise: fast enough that a running clock does not visibly
/// stutter, slow enough that the page costs a third of what it would. Raise it
/// for a page with real animation in it, and expect to pay for that.
fn default_overlay_fps() -> u32 {
    10
}

/// What the supervisor does with a source that has stopped delivering.
///
/// The defaults come from two nights on air, 2026-09-11 and 2026-09-12, when a
/// superimposed source started coming up dead and the mixer rebuilt it every
/// twelve seconds for two hours: 485 rebuilds the first night, 1174 the second,
/// each one cutting the programme to black for the ten or so seconds the
/// browser took to start. Neither number is a repair strategy, it is a loop,
/// and the operator had nothing in the UI to tell him it was running.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StallConfig {
    /// Seconds a source may deliver nothing before its pipeline is rebuilt.
    #[serde(default = "default_restart_after_stall_secs")]
    pub restart_after_secs: u64,
    /// Consecutive rebuilds that did not bring the source back before the
    /// mixer stops trying at full speed. Three is enough to cover the things
    /// that really do heal on a retry (a browser that lost a race with its
    /// profile directory, a page served a 502) and short enough that a fault
    /// which is not going to heal is recognised inside a minute.
    #[serde(default = "default_rebuild_attempts")]
    pub rebuild_attempts: u32,
    /// The delay after that many failures, doubled on each further one.
    #[serde(default = "default_rebuild_backoff_secs")]
    pub rebuild_backoff_secs: u64,
    /// Ceiling for that delay. Five minutes: long enough that a broken source
    /// costs almost nothing, short enough that a fix applied at the far end
    /// is picked up without anyone restarting the mixer.
    #[serde(default = "default_rebuild_backoff_max_secs")]
    pub rebuild_backoff_max_secs: u64,
    /// Hold the last frame of a source being rebuilt on programme, instead of
    /// cutting to the slate for as long as the rebuild takes.
    #[serde(default = "default_true")]
    pub hold_last_frame: bool,
}

fn default_restart_after_stall_secs() -> u64 {
    10
}
fn default_rebuild_attempts() -> u32 {
    3
}
fn default_rebuild_backoff_secs() -> u64 {
    30
}
fn default_rebuild_backoff_max_secs() -> u64 {
    300
}
fn default_true() -> bool {
    true
}

impl Default for StallConfig {
    fn default() -> Self {
        Self {
            restart_after_secs: default_restart_after_stall_secs(),
            rebuild_attempts: default_rebuild_attempts(),
            rebuild_backoff_secs: default_rebuild_backoff_secs(),
            rebuild_backoff_max_secs: default_rebuild_backoff_max_secs(),
            hold_last_frame: default_true(),
        }
    }
}

impl StallConfig {
    /// How long to wait before rebuilding a source that has already failed
    /// `failures` times in a row, or `None` to rebuild at once.
    ///
    /// Pure, so the shape of the curve can be checked without a pipeline.
    pub fn rebuild_delay(&self, failures: u32) -> Option<std::time::Duration> {
        let over = failures.checked_sub(self.rebuild_attempts)?;
        if self.rebuild_backoff_secs == 0 {
            return None;
        }
        // Doubling, in seconds, saturating rather than wrapping: a source left
        // broken overnight reaches the ceiling and stays there.
        let secs = self
            .rebuild_backoff_secs
            .saturating_mul(1u64.checked_shl(over.min(32)).unwrap_or(u64::MAX))
            .min(self.rebuild_backoff_max_secs.max(self.rebuild_backoff_secs));
        Some(std::time::Duration::from_secs(secs))
    }
}

/// Where the ad library lives on the machine running the mixer.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MediaConfig {
    // Every field defaults independently, so `[media]` with only a `dir` set
    // is a complete section. Requiring the rest would be a trap.
    #[serde(default = "default_media_dir")]
    pub dir: String,
    /// How deep to recurse into subdirectories.
    #[serde(default = "default_media_depth")]
    pub max_depth: usize,
    /// Upper bound on how many files to list, so a misconfigured path pointing
    /// at something enormous cannot hang the control server.
    #[serde(default = "default_media_files")]
    pub max_files: usize,
    /// Seconds to spend inspecting one file for its duration.
    #[serde(default = "default_media_probe")]
    pub probe_timeout_secs: u64,
    /// Whether the control port may write files into the library. On by
    /// default: an upload is bounded by the extension list and lands as one
    /// flat name in a directory whose whole purpose is to hold clips nobody
    /// vetted, a far narrower grant than running an exec source.
    #[serde(default = "default_allow_upload")]
    pub allow_upload: bool,
    /// Largest upload accepted, in bytes. The body streams to disk, so this is
    /// about the container's disk, not its memory.
    #[serde(default = "default_max_upload")]
    pub max_upload_bytes: usize,
    /// x264 threads for a conversion, capped so a transcode cannot take every
    /// core from the live programme encoder.
    #[serde(default = "default_convert_threads")]
    pub convert_threads: u32,
}

fn default_media_dir() -> String {
    "media".into()
}
fn default_media_depth() -> usize {
    3
}
fn default_media_files() -> usize {
    500
}
fn default_media_probe() -> u64 {
    3
}
fn default_allow_upload() -> bool {
    true
}
fn default_max_upload() -> usize {
    2 << 30
}
fn default_convert_threads() -> u32 {
    2
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            dir: default_media_dir(),
            max_depth: default_media_depth(),
            max_files: default_media_files(),
            probe_timeout_secs: default_media_probe(),
            allow_upload: default_allow_upload(),
            max_upload_bytes: default_max_upload(),
            convert_threads: default_convert_threads(),
        }
    }
}

/// What to do about the video a website is playing.
///
/// A page that plays a video normally costs a whole CPU core: Chromium decodes
/// every frame in software and repaints the whole page around it, and the
/// result crosses to the mixer as raw frames. Almost all of that is avoidable
/// when the media has an address a decoder can open on its own. The mixer then
/// decodes it on the GPU like any other source and has the browser draw only
/// the page over the top, transparent where the video was.
///
/// It does not always apply. A page that feeds its player from JavaScript, which
/// is what YouTube and most streaming sites do, has no address to hand over, and
/// neither does anything behind DRM. `Auto` looks, uses it when it is there, and
/// silently renders the whole page in the browser when it is not.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Superimpose {
    /// Render everything in the browser. What every source did before this
    /// existed, and still the default.
    #[default]
    Off,
    /// Decode the page's media directly when it can be, and draw the page over
    /// it. Falls back to `Off` for that source when it cannot.
    Auto,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceConfig {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub uri: String,
    /// Seconds without a buffer before the source is treated as dead and its
    /// program pad is faded to the slate.
    #[serde(default = "default_stall_timeout")]
    pub stall_timeout_secs: f64,
    /// Which RTMP client to pull with. See `RtmpClient`.
    #[serde(default)]
    pub rtmp_client: RtmpClient,
    /// Website sources only: whether to decode the page's own video directly
    /// and superimpose the page on it. See `Superimpose`.
    #[serde(default)]
    pub superimpose: Superimpose,
    /// Where the operator's fader for this source sits, 0.0 silent through 1.0
    /// unity to a ceiling of 10.0. Saved with the source so a restart brings
    /// the desk back where it was left rather than resetting every fader to
    /// unity mid-broadcast.
    #[serde(default = "crate::state::unity_gain")]
    pub gain: f64,
    /// Muted by the operator. Saved separately from the fader so that unmuting
    /// after a restart returns the source to the level it had.
    #[serde(default)]
    pub muted: bool,
}

fn default_stall_timeout() -> f64 {
    2.0
}

impl SourceConfig {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

/// Reconnect behaviour differs sharply between an RTMP server you own and a
/// public CDN. Your own server will happily take you back in 100 ms. YouTube
/// and Twitch will throttle or blacklist you for hammering, so the CDN preset
/// backs off much harder and starts slower.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputPolicy {
    /// RTMP server under your control.
    #[default]
    Own,
    /// Public ingest that will punish aggressive reconnects.
    Cdn,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputConfig {
    pub id: String,
    pub uri: String,
    #[serde(default)]
    pub policy: OutputPolicy,
    /// Overrides the policy preset when present.
    #[serde(default)]
    pub reconnect: Option<ReconnectConfig>,
    /// How much encoded data to hold before the muxer. This is the buffer that
    /// makes a short network hiccup invisible to the viewer.
    #[serde(default = "default_queue_secs")]
    pub queue_secs: f64,
}

fn default_queue_secs() -> f64 {
    5.0
}

impl OutputConfig {
    pub fn reconnect_policy(&self) -> ReconnectConfig {
        self.reconnect.unwrap_or_else(|| ReconnectConfig::preset(self.policy))
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct ReconnectConfig {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub multiplier: f64,
}

impl ReconnectConfig {
    pub fn preset(policy: OutputPolicy) -> Self {
        match policy {
            OutputPolicy::Own => {
                Self { initial_delay_ms: 100, max_delay_ms: 2_000, multiplier: 1.6 }
            }
            OutputPolicy::Cdn => {
                Self { initial_delay_ms: 1_000, max_delay_ms: 30_000, multiplier: 2.0 }
            }
        }
    }

    /// Delay before attempt number `attempt`, counting from zero.
    pub fn delay_for(&self, attempt: u32) -> std::time::Duration {
        let scaled =
            self.initial_delay_ms as f64 * self.multiplier.powi(attempt.min(16) as i32);
        let ms = scaled.min(self.max_delay_ms as f64) as u64;
        std::time::Duration::from_millis(ms)
    }
}

/// Sources and outputs managed at runtime, stored beside the config file.
#[derive(Debug, Deserialize)]
struct StoredRuntime {
    /// Absent means "not managed here", so the config file's list stands.
    #[serde(default)]
    sources: Option<Vec<SourceConfig>>,
    #[serde(default)]
    outputs: Option<Vec<OutputConfig>>,
}

impl Config {
    /// Where runtime source and output changes are saved for a config path.
    pub fn runtime_store_path(config: &Path) -> std::path::PathBuf {
        let mut name = config.file_stem().unwrap_or_default().to_os_string();
        name.push(".runtime.toml");
        config.with_file_name(name)
    }

    /// The control token in force: the environment first, then the config
    /// file. Empty strings count as unset, so `LIVEBOXMIX_TOKEN=` in a unit
    /// file does not lock everyone out with a token nobody can type.
    pub fn token(&self) -> Option<String> {
        let present = |t: String| {
            let t = t.trim().to_string();
            (!t.is_empty()).then_some(t)
        };
        std::env::var("LIVEBOXMIX_TOKEN")
            .ok()
            .and_then(present)
            .or_else(|| self.control.token.clone().and_then(present))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let mut cfg: Config = toml::from_str(&raw)
            .with_context(|| format!("parsing config {}", path.display()))?;

        // Once the UI has managed sources, its list wins. Merging the two would
        // mean a source deleted in the UI reappearing on the next restart.
        let store = Self::runtime_store_path(path);
        if store.exists() {
            let raw = std::fs::read_to_string(&store)
                .with_context(|| format!("reading {}", store.display()))?;
            let stored: StoredRuntime = toml::from_str(&raw)
                .with_context(|| format!("parsing {}", store.display()))?;
            if let Some(sources) = stored.sources {
                tracing::info!(
                    path = %store.display(), count = sources.len(),
                    "using sources managed at runtime"
                );
                cfg.sources = sources;
            }
            if let Some(outputs) = stored.outputs {
                tracing::info!(
                    path = %store.display(), count = outputs.len(),
                    "using outputs managed at runtime"
                );
                cfg.outputs = outputs;
            }
        }

        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(self.canvas.width > 0 && self.canvas.height > 0, "canvas dimensions must be positive");
        anyhow::ensure!(self.canvas.width % 2 == 0 && self.canvas.height % 2 == 0, "canvas dimensions must be even for 4:2:0 chroma");
        anyhow::ensure!(self.canvas.fps > 0, "canvas fps must be positive");
        anyhow::ensure!(matches!(self.canvas.channels, 1 | 2), "only mono and stereo are supported");
        anyhow::ensure!(
            (1..=100).contains(&self.multiview.jpeg_quality),
            "multiview.jpeg_quality must be between 1 and 100"
        );
        anyhow::ensure!(self.multiview.fps > 0, "multiview.fps must be positive");

        let mut seen = std::collections::HashSet::new();
        for s in &self.sources {
            anyhow::ensure!(seen.insert(&s.id), "duplicate source id {:?}", s.id);
        }
        let mut seen = std::collections::HashSet::new();
        for o in &self.outputs {
            anyhow::ensure!(seen.insert(&o.id), "duplicate output id {:?}", o.id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdn_backoff_grows_and_saturates() {
        let p = ReconnectConfig::preset(OutputPolicy::Cdn);
        assert_eq!(p.delay_for(0).as_millis(), 1_000);
        assert_eq!(p.delay_for(1).as_millis(), 2_000);
        assert_eq!(p.delay_for(2).as_millis(), 4_000);
        // Saturates at max_delay rather than growing without bound.
        assert_eq!(p.delay_for(30).as_millis(), 30_000);
    }

    #[test]
    fn own_server_reconnects_fast() {
        let p = ReconnectConfig::preset(OutputPolicy::Own);
        assert!(p.delay_for(0).as_millis() <= 100);
        assert!(p.delay_for(20).as_millis() <= 2_000);
    }

    #[test]
    fn auto_falls_back_but_a_pinned_client_does_not() {
        assert_eq!(RtmpClient::Auto.first_element(), "rtmp2src");
        assert_eq!(RtmpClient::Auto.fallback_element(), Some("rtmpsrc"));
        // Pinning a client means the operator decided; never second-guess it.
        assert_eq!(RtmpClient::Rtmp2.first_element(), "rtmp2src");
        assert_eq!(RtmpClient::Rtmp2.fallback_element(), None);
        assert_eq!(RtmpClient::Librtmp.first_element(), "rtmpsrc");
        assert_eq!(RtmpClient::Librtmp.fallback_element(), None);
    }

    /// A section that names only what it cares about must be complete. Serde
    /// otherwise rejects the whole file for a field the operator never heard of.
    #[test]
    fn running_commands_is_off_unless_asked_for() {
        // The default must be closed: an exec source is code execution for
        // anyone who can reach the control port.
        let cfg: Config = toml::from_str("").unwrap();
        assert!(!cfg.security.allow_exec_sources);
        let cfg: Config =
            toml::from_str("[security]\nallow_exec_sources = true\n").unwrap();
        assert!(cfg.security.allow_exec_sources);
    }

    #[test]
    fn a_partial_media_section_is_accepted() {
        let cfg: Config = toml::from_str("[media]\ndir = \"/srv/ads\"\n").unwrap();
        assert_eq!(cfg.media.dir, "/srv/ads");
        assert_eq!(cfg.media.max_depth, default_media_depth());
        assert_eq!(cfg.media.max_files, default_media_files());

        // And omitting the section entirely is fine too.
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.media.dir, "media");
    }

    /// A serde field default only runs when the field's own struct is being
    /// deserialised. A config with no `[browser]` section never gets that far:
    /// the section falls back to `BrowserConfig::default()`, and while that was
    /// derived it handed out a zero here, which reaches the sidecar as
    /// `--fps 0`. Both routes must arrive at the same number.
    #[test]
    fn the_overlay_frame_rate_defaults_the_same_either_way() {
        let missing: Config = toml::from_str("").unwrap();
        let present: Config = toml::from_str("[browser]
").unwrap();
        let partial: Config = toml::from_str("[browser]
sidecar = \"/opt/b\"\n").unwrap();
        assert_eq!(missing.browser.overlay_fps, default_overlay_fps());
        assert_eq!(present.browser.overlay_fps, default_overlay_fps());
        assert_eq!(partial.browser.overlay_fps, default_overlay_fps());
        assert!(default_overlay_fps() > 0, "zero would stop the page painting");

        // And it is still overridable, which is the point of it being config.
        let set: Config = toml::from_str("[browser]\noverlay_fps = 25\n").unwrap();
        assert_eq!(set.browser.overlay_fps, 25);
    }

    /// The loop this replaces rebuilt a dead source every twelve seconds for
    /// two hours. The first few attempts still go at full speed, because some
    /// failures really are transient; after that the delay doubles and stops
    /// at the ceiling.
    #[test]
    fn rebuilds_run_free_then_back_off_and_settle() {
        let s = StallConfig::default();
        assert_eq!(s.rebuild_delay(0), None);
        assert_eq!(s.rebuild_delay(2), None);
        assert_eq!(s.rebuild_delay(3), Some(std::time::Duration::from_secs(30)));
        assert_eq!(s.rebuild_delay(4), Some(std::time::Duration::from_secs(60)));
        assert_eq!(s.rebuild_delay(5), Some(std::time::Duration::from_secs(120)));
        assert_eq!(s.rebuild_delay(6), Some(std::time::Duration::from_secs(240)));
        // Capped, and it stays capped however long it has been broken.
        assert_eq!(s.rebuild_delay(7), Some(std::time::Duration::from_secs(300)));
        assert_eq!(s.rebuild_delay(600), Some(std::time::Duration::from_secs(300)));
    }

    /// Two hours of rebuilding every twelve seconds is 600 attempts. Under
    /// this policy the same two hours is a couple of dozen, which is the
    /// difference between a leak that fills a disk and one nobody notices.
    #[test]
    fn an_hours_backoff_is_a_handful_of_attempts() {
        let s = StallConfig::default();
        let mut elapsed = std::time::Duration::ZERO;
        let mut attempts = 0u32;
        while elapsed < std::time::Duration::from_secs(2 * 3600) {
            elapsed += s.rebuild_delay(attempts).unwrap_or_default();
            attempts += 1;
        }
        assert!(attempts < 40, "two hours took {attempts} rebuilds");
    }

    /// Zero turns the backoff off, for an operator who wants the old
    /// behaviour back rather than a mixer that has decided to give up.
    #[test]
    fn a_zero_backoff_never_waits() {
        let s = StallConfig { rebuild_backoff_secs: 0, ..Default::default() };
        assert_eq!(s.rebuild_delay(50), None);
    }

    #[test]
    fn a_partial_stall_section_is_accepted() {
        let cfg: Config = toml::from_str("[stall]\nrebuild_attempts = 1\n").unwrap();
        assert_eq!(cfg.stall.rebuild_attempts, 1);
        assert_eq!(cfg.stall.rebuild_backoff_secs, default_rebuild_backoff_secs());
        assert!(cfg.stall.hold_last_frame);
        let missing: Config = toml::from_str("").unwrap();
        assert_eq!(missing.stall.restart_after_secs, default_restart_after_stall_secs());
        assert!(missing.stall.hold_last_frame);
    }

    #[test]
    fn odd_canvas_is_rejected() {
        let mut cfg = Config {
            canvas: Canvas { width: 1921, height: 1080, fps: 30, sample_rate: 48000, channels: 2 },
            program: Default::default(),
            multiview: Default::default(),
            control: Default::default(),
            hardware: Default::default(),
            media: Default::default(),
            security: Default::default(),
            browser: Default::default(),
            stall: Default::default(),
            sources: vec![],
            outputs: vec![],
        };
        assert!(cfg.validate().is_err());
        cfg.canvas.width = 1920;
        assert!(cfg.validate().is_ok());
    }
}
