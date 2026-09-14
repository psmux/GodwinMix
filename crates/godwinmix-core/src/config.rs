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
    pub snapshot: SnapshotConfig,
    #[serde(default)]
    pub control: ControlConfig,
    #[serde(default)]
    pub hardware: HardwareConfig,
    /// Additions and overrides to the shipped codec catalogue. An entry here
    /// whose id matches a shipped one replaces it; anything else is appended.
    /// See `codecs.toml` for the shape and `docs/how-to/add-a-codec-entry.md`
    /// for a worked example.
    #[serde(default)]
    pub codecs: crate::catalogue::Catalogue,
    #[serde(default)]
    pub media: MediaConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    /// The rules that stand in front of every take: the minimum hold, the
    /// rate limit, the flash guard and the operator watchdog. See
    /// `crate::safety` and `docs/reference/safety.md`.
    #[serde(default)]
    pub safety: crate::safety::SafetyConfig,
    #[serde(default)]
    pub browser: BrowserConfig,
    #[serde(default)]
    pub stall: StallConfig,
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    #[serde(default)]
    pub outputs: Vec<OutputConfig>,
    /// Filters attached at startup. Each one names a built in or plugin
    /// provided filter and where it goes.
    #[serde(default)]
    pub filters: Vec<FilterConfig>,
    /// Settings belonging to a plugin, one table per plugin name. The core
    /// never reads inside these; it hands `[plugins.ndi]` to the plugin called
    /// `ndi` and nothing else sees it.
    #[serde(default)]
    pub plugins: std::collections::BTreeMap<String, Params>,
    /// Several credentials, each with its own scopes. The single
    /// `[control] token` still works and still carries everything; this is
    /// for a show that wants an agent's token to be able to take and not to
    /// remove. See `api::scope` and `Config::tokens`.
    #[serde(default)]
    pub tokens: Vec<TokenConfig>,
    /// Every other top level table. Without this a plugin's section was
    /// silently dropped, which is the closed schema the audit named.
    #[serde(flatten, default)]
    pub extra: std::collections::BTreeMap<String, toml::Value>,
}

/// A plugin's own settings, as written in `params = { .. }` or in
/// `[plugins.<name>]`. A TOML table, uninterpreted by the core.
pub type Params = toml::Table;

/// Where a filter goes. Either on one source, on the input or the programme
/// side of the proxy boundary, or on the programme itself.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FilterConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub type_id: String,
    #[serde(default)]
    pub attach: FilterAttach,
    #[serde(default)]
    pub params: Params,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FilterAttach {
    /// The source this filter belongs to, when it belongs to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `input` puts it before the proxy boundary, where it also reaches the
    /// thumbnail; `programme` puts it on this source's programme branch only.
    #[serde(default)]
    pub side: FilterAttachSide,
    /// Set instead of `source` to filter the whole programme.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub programme: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterAttachSide {
    /// Between the source's canvas capsfilter and its tee: the programme and
    /// the thumbnail both see it.
    #[default]
    Input,
    /// Between this source's programme queue and the compositor pad.
    Programme,
}


/// One row of the `[[tokens]]` table.
///
/// ```toml
/// [[tokens]]
/// id = "studio-agent"
/// secret = "..."
/// scopes = ["read", "operate"]
/// confirm = "required"     # destructive calls need a confirm round trip
/// rehearsal = true         # only a core started with --rehearsal accepts it
/// profile = "minimal"      # which MCP tool surface this token is meant for
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokenConfig {
    /// Legible, and recorded against every take in `program.history`.
    pub id: String,
    pub secret: String,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<godwinmix_protocol::scope::Scope>,
    #[serde(default)]
    pub confirm: godwinmix_protocol::scope::ConfirmPolicy,
    #[serde(default)]
    pub rehearsal: bool,
    #[serde(default)]
    pub profile: godwinmix_protocol::scope::Profile,
    /// This credential belongs to an unattended agent. Its `safety` override
    /// may then only make the core's limits harder, never easier.
    #[serde(default)]
    pub agent: bool,
    /// `safety = { min_hold_ms = 2000 }` and the like, overriding `[safety]`
    /// for this token alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety: Option<godwinmix_protocol::scope::TokenSafety>,
}

/// A token that names no scopes can read. Anything more has to be asked for,
/// because the cost of a token that quietly carries `admin` is the whole
/// point of having the table.
fn default_scopes() -> Vec<godwinmix_protocol::scope::Scope> {
    vec![godwinmix_protocol::scope::Scope::Read]
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

/// The operator's mosaic.
///
/// Every field has a default, so `[multiview]\nenabled = false` on its own is
/// a valid section: turning a subsystem off must not oblige anybody to write
/// out the settings of the thing they are turning off.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MultiviewConfig {
    /// False removes the mosaic entirely: no pipeline, no encoder, no
    /// thumbnail end on any source, and the snapshot routes answer 404. See
    /// `multiview.rs`. True does not mean it is running; it means a client may
    /// ask for it.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Size of the whole mosaic, not of one cell. What a client that does not
    /// ask for a size gets.
    #[serde(default = "default_multiview_width")]
    pub width: i32,
    #[serde(default = "default_multiview_height")]
    pub height: i32,
    #[serde(default = "default_multiview_fps")]
    pub fps: i32,
    /// JPEG quality, 1 to 100. The mosaic exists so an operator can tell what
    /// is in frame and pick a camera, not to judge picture quality, so this is
    /// deliberately modest. Bandwidth is roughly width * height * fps * q.
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u32,
    /// Include a program return cell in the grid.
    #[serde(default = "yes")]
    pub include_program: bool,
    /// Seconds to keep the mosaic up after the last subscriber leaves. A
    /// browser reloading its page comes back inside this and pays nothing;
    /// zero tears the pipeline down the moment the last client goes.
    #[serde(default = "default_multiview_linger_secs")]
    pub linger_secs: u64,
}

fn yes() -> bool {
    true
}
fn default_multiview_width() -> i32 {
    960
}
fn default_multiview_height() -> i32 {
    540
}
fn default_multiview_fps() -> i32 {
    8
}
fn default_jpeg_quality() -> u32 {
    60
}
fn default_multiview_linger_secs() -> u64 {
    2
}

impl Default for MultiviewConfig {
    fn default() -> Self {
        // 960x540 at 8 fps and quality 60 costs roughly 2 Mbit/s, which a
        // remote operator on a hotel connection can actually receive.
        Self {
            enabled: yes(),
            width: default_multiview_width(),
            height: default_multiview_height(),
            fps: default_multiview_fps(),
            jpeg_quality: default_jpeg_quality(),
            include_program: yes(),
            linger_secs: default_multiview_linger_secs(),
        }
    }
}

/// Stills and the motion tracker that reads them.
///
/// The tracker decodes a mosaic frame per tick, which is the most expensive
/// thing the core does on behalf of a client that is not watching, so it runs
/// only while something is actually asking: a snapshot request, an
/// `agent.state` read, or a subscriber with agent thresholds. The numbers here
/// are the ones 09 section 3 commits to.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SnapshotConfig {
    /// False removes the tracker and the snapshot routes entirely. The routes
    /// then answer 404 naming this switch, and `agent.state` reports
    /// `motion: null`.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// What a request without `?width=` gets. 320x180 is 84 tokens to a
    /// vision model against 2,691 for a 1080p frame, and it answers "is
    /// anybody in the shot" about as well.
    #[serde(default = "default_snapshot_width")]
    pub default_width: u32,
    /// Seconds a client must wait between snapshots unless it passes
    /// `?force=true`. Zero turns the limit off.
    #[serde(default = "default_snapshot_min_interval_secs")]
    pub min_interval_secs: u64,
    /// Widths above this are refused unless the request passes
    /// `?allow_large=true`. A 1080p still costs a vision model thirty times
    /// what a 320 wide one does, so asking for it should be deliberate.
    #[serde(default = "default_snapshot_max_width")]
    pub max_width: u32,
    /// Seconds the tracker keeps following the mosaic after the last request.
    /// An agent polling every five seconds keeps it up; one that stops gets
    /// its CPU back.
    #[serde(default = "default_snapshot_idle_secs")]
    pub idle_secs: u64,
}

fn default_snapshot_width() -> u32 {
    320
}
fn default_snapshot_min_interval_secs() -> u64 {
    5
}
fn default_snapshot_max_width() -> u32 {
    1280
}
fn default_snapshot_idle_secs() -> u64 {
    15
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            enabled: yes(),
            default_width: default_snapshot_width(),
            min_interval_secs: default_snapshot_min_interval_secs(),
            max_width: default_snapshot_max_width(),
            idle_secs: default_snapshot_idle_secs(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ControlConfig {
    pub bind: String,
    /// Directory of static UI assets. Falls back to the embedded page. Point
    /// it at the repository's `ui/` to edit the page without rebuilding.
    pub ui_dir: Option<String>,
    /// Where plugins live. Each one's `ui/` directory is served at
    /// `/plugins/<name>/ui/`, which is how a plugin adds a panel to the web
    /// UI. Defaults to `~/.godwinmix/plugins`.
    #[serde(default)]
    pub plugins_dir: Option<String>,
    /// Bearer token every `/api/*` request and the WebSocket must carry.
    /// Unset means the control port is open to whoever can reach it, which is
    /// how it has always worked and is fine behind a firewall. The
    /// `GODWINMIX_TOKEN` environment variable overrides this, so a deployment
    /// can keep the secret out of the config file. `LIVEBOXMIX_TOKEN` is
    /// accepted for one release and warns. See `Config::token`.
    #[serde(default)]
    pub token: Option<String>,
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self { bind: "0.0.0.0:8080".into(), ui_dir: None, plugins_dir: None, token: None }
    }
}

/// Hardware acceleration preference. `Auto` picks the highest ranked
/// catalogue entry whose elements are installed; the named variants pin an
/// `accel` and fail loudly if no entry with it is present, which is what you
/// want on a server you control.
///
/// These names are the `accel` field in `codecs.toml`. Adding a vendor means
/// adding entries there and, if it is one an operator should be able to pin,
/// one variant here.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Accel {
    #[default]
    Auto,
    Nvidia,
    Va,
    Qsv,
    Amf,
    VideoToolbox,
    MediaFoundation,
    D3d11,
    D3d12,
    Cuda,
    Gl,
    Vulkan,
    V4l2,
    Software,
}

impl Accel {
    /// The `accel` string this matches in the catalogue, or None for `Auto`,
    /// which matches everything.
    pub fn name(self) -> Option<&'static str> {
        Some(match self {
            Accel::Auto => return None,
            Accel::Nvidia => "nvidia",
            Accel::Va => "va",
            Accel::Qsv => "qsv",
            Accel::Amf => "amf",
            Accel::VideoToolbox => "videotoolbox",
            Accel::MediaFoundation => "mediafoundation",
            Accel::D3d11 => "d3d11",
            Accel::D3d12 => "d3d12",
            Accel::Cuda => "cuda",
            Accel::Gl => "gl",
            Accel::Vulkan => "vulkan",
            Accel::V4l2 => "v4l2",
            Accel::Software => "software",
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HardwareConfig {
    #[serde(default)]
    pub decode: Accel,
    #[serde(default)]
    pub encode: Accel,
    /// Which compositor and conversion backend to draw the canvas on. `Auto`
    /// keeps the software compositor until a GPU entry carries a `verified`
    /// record for this platform; pin one to run the soak that earns it.
    #[serde(default)]
    pub graphics: Accel,
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

    /// How this setting is spelled in `params.client`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Rtmp2 => Self::RTMP2_ELEMENT,
            Self::Librtmp => Self::LIBRTMP_ELEMENT,
        }
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
/// The preferred renderer is `godwinmix-browser`, the CEF sidecar in
/// `browser/`: a full Chromium drawing off screen, handing over raw frames and
/// PCM with no encoder in between. When it is found, every `web+` source runs
/// through it. When it is not, `web+` falls back to GStreamer's `wpesrc`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BrowserConfig {
    /// Path to `godwinmix-browser`. Unset means: look next to this executable,
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
    /// The plugin qualified provide id: `file/source`, `rtmp/source`,
    /// `browser/source` and so on. Absent means "work it out from the URI",
    /// which is what every config written before this said.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
    /// Still accepted, and still how most sources are written. It is resolved
    /// to a `type` by scheme and rank and kept in `params.uri`.
    #[serde(default)]
    pub uri: String,
    /// What this source's kind makes of it. The core does not read inside.
    #[serde(default, skip_serializing_if = "Params::is_empty")]
    pub params: Params,
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
    /// Every key the core does not know. They reach the source's kind through
    /// `effective_params` rather than being dropped on the floor.
    #[serde(flatten, default)]
    pub extra: std::collections::BTreeMap<String, toml::Value>,
}

fn default_stall_timeout() -> f64 {
    2.0
}

impl SourceConfig {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    /// What to show an operator as this source's address.
    ///
    /// Most sources have a `uri` and that is it. One written as `type` plus
    /// `params` may have its address inside the params, and one that has no
    /// address at all (a capture card, a test pattern) has only its kind to
    /// show. An empty string masked to an ellipsis told the operator nothing.
    pub fn display_uri(&self) -> String {
        if !self.uri.trim().is_empty() {
            return self.uri.clone();
        }
        if let Some(u) = self.params.get("uri").and_then(|v| v.as_str()) {
            return u.to_string();
        }
        self.type_id.clone().unwrap_or_default()
    }

    /// A source with nothing but an id and an address, for a caller building
    /// one by hand.
    pub fn bare(id: &str, uri: &str) -> Self {
        Self {
            id: id.to_string(),
            name: None,
            type_id: None,
            uri: uri.to_string(),
            params: Params::new(),
            stall_timeout_secs: default_stall_timeout(),
            rtmp_client: RtmpClient::default(),
            superimpose: Superimpose::default(),
            gain: crate::state::unity_gain(),
            muted: false,
            extra: Default::default(),
        }
    }

    /// The params the kind actually receives: what was written in `params`,
    /// plus the legacy fields the migration table maps in, plus anything the
    /// core did not recognise.
    ///
    /// Every mapped field warns once, naming the key to write instead, so an
    /// operator who reads their logs can move over before the old names go.
    pub fn effective_params(&self) -> Params {
        let mut out = self.params.clone();
        // `uri` is not a migration, it is the ordinary way to write a source,
        // so it is carried without a warning.
        if !self.uri.trim().is_empty() && !out.contains_key("uri") {
            out.insert("uri".into(), toml::Value::String(self.uri.clone()));
        }
        // The migration table from the plugin architecture, one row per line.
        // Each mapped field warns once, naming the key to write instead.
        let mapped: [(&str, &str, Option<toml::Value>); 2] = [
            (
                "rtmp_client",
                "client",
                (self.rtmp_client != RtmpClient::default())
                    .then(|| toml::Value::String(self.rtmp_client.as_str().into())),
            ),
            (
                "superimpose",
                "superimpose",
                (self.superimpose != Superimpose::default())
                    .then(|| toml::Value::String("auto".into())),
            ),
        ];
        for (from, key, value) in mapped {
            let Some(value) = value else { continue };
            if out.contains_key(key) {
                continue;
            }
            tracing::warn!(
                source = %self.id,
                "`{from}` on a source is now `params.{key}`; the old key will not be read after this release"
            );
            out.insert(key.to_string(), value);
        }
        for (k, v) in &self.extra {
            out.entry(k.clone()).or_insert_with(|| v.clone());
        }
        out
    }

    /// Check `params` against the kind this source resolves to. A bad param
    /// names the field and what it accepts, rather than failing at build time
    /// inside GStreamer.
    pub fn validate_params(&self) -> anyhow::Result<()> {
        let provide = match crate::plugin::source::resolve_config(self) {
            Ok(p) => p,
            Err(e) if names_a_plugin(self.type_id.as_deref()) => {
                // A type from a plugin this build does not carry is not a
                // config error: the plugin loader supplies it, or the source
                // is reported as needing that plugin when it is built. A
                // preset written for a plugin must load on a core that has
                // not installed it yet.
                tracing::warn!(source = %self.id, "{e}; the source will need that plugin installed");
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let params = self.effective_params();
        match provide.manifest.plugin {
            "rtmp" => crate::plugin::kinds::rtmp::validate(&params),
            "hls" => crate::plugin::kinds::live::validate(&params),
            "file" => crate::plugin::kinds::file::validate(&params),
            "exec" => crate::plugin::kinds::exec::validate(&params),
            "browser" => crate::plugin::kinds::browser::validate(&params),
            "layered" => crate::plugin::kinds::layered::validate(&params),
            "test" => crate::plugin::kinds::testsrc::validate(&params),
            _ => Ok(()),
        }
    }
}

/// Whether a `type` names a plugin provide (`ndi/source`) rather than nothing
/// at all. A bare word with no slash is a typo; a qualified id is a plugin
/// this build may not carry, which the loader decides, not the parser.
fn names_a_plugin(type_id: Option<&str>) -> bool {
    type_id
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| t.contains('/'))
        .unwrap_or(false)
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
    /// The plugin qualified provide id: `rtmp/output`, `srt/output`. Absent
    /// means "work it out from the URI".
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
    #[serde(default)]
    pub uri: String,
    /// What this output's kind makes of it.
    #[serde(default, skip_serializing_if = "Params::is_empty")]
    pub params: Params,
    #[serde(default)]
    pub policy: OutputPolicy,
    /// Overrides the policy preset when present.
    #[serde(default)]
    pub reconnect: Option<ReconnectConfig>,
    /// How much encoded data to hold before the muxer. This is the buffer that
    /// makes a short network hiccup invisible to the viewer.
    #[serde(default = "default_queue_secs")]
    pub queue_secs: f64,
    /// Every key the core does not know, handed to the output's kind.
    #[serde(flatten, default)]
    pub extra: std::collections::BTreeMap<String, toml::Value>,
}

fn default_queue_secs() -> f64 {
    5.0
}

impl OutputConfig {
    pub fn reconnect_policy(&self) -> ReconnectConfig {
        self.reconnect.unwrap_or_else(|| ReconnectConfig::preset(self.policy))
    }

    /// An output with nothing but an id and an address.
    pub fn bare(id: &str, uri: &str) -> Self {
        Self {
            id: id.to_string(),
            type_id: None,
            uri: uri.to_string(),
            params: Params::new(),
            policy: OutputPolicy::default(),
            reconnect: None,
            queue_secs: default_queue_secs(),
            extra: Default::default(),
        }
    }

    /// The params the output's kind receives, with `uri` carried in.
    pub fn effective_params(&self) -> Params {
        let mut out = self.params.clone();
        if !self.uri.trim().is_empty() && !out.contains_key("uri") {
            out.insert("uri".into(), toml::Value::String(self.uri.clone()));
        }
        for (k, v) in &self.extra {
            out.entry(k.clone()).or_insert_with(|| v.clone());
        }
        out
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

/// The value of a `GODWINMIX_*` variable, accepting the `LIVEBOXMIX_*` name
/// the product carried before the rename.
///
/// The old name works for one release and says so in the log the first time it
/// is used, because a deployment that sets it in a unit file or a container's
/// environment should not lose its token to a rename it did not make. Both
/// this function and the fallback go away in the release after 0.2.
pub fn env_var(suffix: &str) -> Option<String> {
    if let Ok(v) = std::env::var(format!("GODWINMIX_{suffix}")) {
        return Some(v);
    }
    match std::env::var(format!("LIVEBOXMIX_{suffix}")) {
        Ok(v) => {
            tracing::warn!(
                "LIVEBOXMIX_{suffix} is the old name and will stop working after this release: \
                 set GODWINMIX_{suffix} instead"
            );
            Some(v)
        }
        Err(_) => None,
    }
}

/// The config file to read, given the one that was asked for.
///
/// Normally that is the one that was asked for. The exception is the rename:
/// a box that has `liveboxmix.toml` beside it and no `godwinmix.toml` is a box
/// that was working yesterday, and it keeps working for one release. The
/// runtime store follows the stem of whichever file is used, so a mixer that
/// falls back keeps writing `liveboxmix.runtime.toml` and its sources stay
/// where it left them. Dropped in the release after 0.2.
pub fn path_in_force(asked: &Path) -> std::path::PathBuf {
    if asked.exists() {
        return asked.to_path_buf();
    }
    let Some(legacy) = asked
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("godwinmix"))
        .map(|rest| asked.with_file_name(format!("liveboxmix{rest}")))
    else {
        return asked.to_path_buf();
    };
    if legacy.exists() {
        tracing::warn!(
            path = %legacy.display(),
            "no {} here, reading the LiveboxMix config instead. Rename it: the old name will not be read after this release",
            asked.display()
        );
        return legacy;
    }
    asked.to_path_buf()
}

impl Config {
    /// Where runtime source and output changes are saved for a config path.
    pub fn runtime_store_path(config: &Path) -> std::path::PathBuf {
        let mut name = config.file_stem().unwrap_or_default().to_os_string();
        name.push(".runtime.toml");
        config.with_file_name(name)
    }

    /// The control token in force: the environment first, then the config
    /// file. Empty strings count as unset, so `GODWINMIX_TOKEN=` in a unit
    /// file does not lock everyone out with a token nobody can type.
    pub fn token(&self) -> Option<String> {
        let present = |t: String| {
            let t = t.trim().to_string();
            (!t.is_empty()).then_some(t)
        };
        env_var("TOKEN")
            .and_then(present)
            .or_else(|| self.control.token.clone().and_then(present))
    }

    /// Every credential in force: the `[[tokens]]` table, plus the single
    /// bearer token when one is set. A deployment with neither leaves the
    /// control port open, which is how it has always worked.
    pub fn tokens(&self, rehearsal_core: bool) -> godwinmix_protocol::scope::Tokens {
        let mut entries: Vec<godwinmix_protocol::scope::Token> = self
            .tokens
            .iter()
            .filter(|t| !t.secret.trim().is_empty())
            .map(|t| godwinmix_protocol::scope::Token {
                id: t.id.clone(),
                secret: t.secret.trim().to_string(),
                scopes: t.scopes.clone(),
                confirm: t.confirm,
                rehearsal: t.rehearsal,
                profile: t.profile,
                agent: t.agent,
                safety: t.safety,
            })
            .collect();
        if let Some(secret) = self.token() {
            entries.push(godwinmix_protocol::scope::Token::legacy(&secret));
        }
        godwinmix_protocol::scope::Tokens::new(entries, rehearsal_core)
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
        anyhow::ensure!(
            self.multiview.width > 0 && self.multiview.height > 0,
            "multiview dimensions must be positive"
        );
        anyhow::ensure!(
            self.snapshot.default_width >= 16,
            "snapshot.default_width must be at least 16; 320 is the documented default"
        );
        anyhow::ensure!(
            self.snapshot.max_width >= self.snapshot.default_width,
            "snapshot.max_width ({}) is below snapshot.default_width ({}), so every \
             default request would be refused",
            self.snapshot.max_width,
            self.snapshot.default_width
        );
        // Per kind validation of `params`. A bad param names the field and the
        // values it accepts, at startup, rather than failing somewhere inside
        // GStreamer once the show has begun.
        for s in &self.sources {
            s.validate_params()
                .with_context(|| format!("source {}", s.id))?;
        }
        for o in &self.outputs {
            let provide = match crate::plugin::output::resolve_config(o) {
                Ok(p) => p,
                Err(e) if names_a_plugin(o.type_id.as_deref()) => {
                    tracing::warn!(output = %o.id, "{e}; the output will need that plugin installed");
                    continue;
                }
                Err(e) => return Err(e).with_context(|| format!("output {}", o.id)),
            };
            let params = o.effective_params();
            let checked = match provide.manifest.plugin {
                "rtmp" => crate::plugin::outputs::rtmp::validate(&params),
                "srt" => crate::plugin::outputs::srt::validate(&params),
                _ => Ok(()),
            };
            checked.with_context(|| format!("output {}", o.id))?;
        }
        for f in &self.filters {
            anyhow::ensure!(
                f.attach.source.is_some() || f.attach.programme,
                "filter {} must say where it goes: attach = {{ source = \"cam1\" }} \
                 or attach = {{ programme = true }}",
                f.id
            );
            anyhow::ensure!(
                !(f.attach.source.is_some() && f.attach.programme),
                "filter {} names a source and the programme; it can only go on one",
                f.id
            );
            if f.type_id == crate::plugin::filters::chroma::MANIFEST.provide_id() {
                crate::plugin::filters::chroma::validate(&f.params)
                    .with_context(|| format!("filter {}", f.id))?;
            }
        }

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

    /// A box that was running LiveboxMix yesterday has `liveboxmix.toml` and
    /// nothing else. It keeps running for one release, and its runtime store
    /// keeps the same stem so the sources it was given are still found.
    #[test]
    fn the_old_config_name_is_read_when_the_new_one_is_absent() {
        let dir = std::env::temp_dir().join(format!("gmx-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let asked = dir.join("godwinmix.toml");
        let legacy = dir.join("liveboxmix.toml");

        // Neither file: what was asked for, so the error names the new name.
        assert_eq!(path_in_force(&asked), asked);

        std::fs::write(&legacy, "").unwrap();
        assert_eq!(path_in_force(&asked), legacy);
        assert_eq!(
            Config::runtime_store_path(&path_in_force(&asked)),
            dir.join("liveboxmix.runtime.toml")
        );

        // Once the new name exists it wins, whatever is beside it.
        std::fs::write(&asked, "").unwrap();
        assert_eq!(path_in_force(&asked), asked);

        // A path that is neither name is passed through untouched.
        let other = dir.join("studio.toml");
        assert_eq!(path_in_force(&other), other);

        let _ = std::fs::remove_dir_all(&dir);
    }

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

    /// Turning a subsystem off is one line. Nobody should have to write out
    /// the width of a mosaic they have just disabled.
    #[test]
    fn the_switches_are_one_line_each() {
        let cfg: Config =
            toml::from_str("[multiview]\nenabled = false\n[snapshot]\nenabled = false\n").unwrap();
        assert!(!cfg.multiview.enabled);
        assert!(!cfg.snapshot.enabled);
        assert_eq!(cfg.multiview.width, default_multiview_width());
        assert_eq!(cfg.multiview.linger_secs, 2);
        assert_eq!(cfg.snapshot.default_width, 320);
        assert_eq!(cfg.snapshot.min_interval_secs, 5);
        assert_eq!(cfg.snapshot.max_width, 1280);
        cfg.validate().unwrap();

        // Both default to on, so an existing config file is unchanged.
        let bare: Config = toml::from_str("").unwrap();
        assert!(bare.multiview.enabled);
        assert!(bare.snapshot.enabled);
    }

    #[test]
    fn a_snapshot_ceiling_below_the_default_is_rejected() {
        let cfg: Config =
            toml::from_str("[snapshot]\ndefault_width = 640\nmax_width = 320\n").unwrap();
        let err = format!("{:#}", cfg.validate().unwrap_err());
        assert!(err.contains("max_width"), "unhelpful error: {err}");
    }

    /// The example config is what a new operator starts from and what
    /// `--example-config` prints. It has to parse and validate, or the first
    /// thing anyone does with this program fails.
    #[test]
    fn the_example_config_parses_and_validates() {
        let cfg: Config = toml::from_str(include_str!("../../../godwinmix.example.toml"))
            .expect("the example config parses");
        cfg.validate().expect("the example config validates");
    }

    #[test]
    fn a_plugins_table_and_an_unknown_section_both_survive_the_load() {
        let cfg: Config = toml::from_str(
            r#"
            [plugins.ndi]
            discovery_interval_secs = 5

            [something_no_core_version_knows]
            a = 1
            "#,
        )
        .expect("unknown sections are kept, not refused");
        assert_eq!(
            cfg.plugins["ndi"]["discovery_interval_secs"].as_integer(),
            Some(5),
            "a plugin's own settings must reach it"
        );
        assert!(
            cfg.extra.contains_key("something_no_core_version_knows"),
            "an unknown top level table must not be dropped on the floor"
        );
    }

    #[test]
    fn the_old_field_names_are_carried_into_params() {
        let cfg: Config = toml::from_str(
            r#"
            [[sources]]
            id = "cam1"
            uri = "rtmp://host/live/cam1"
            rtmp_client = "librtmp"

            [[sources]]
            id = "page"
            uri = "web+https://example.com/score"
            superimpose = "auto"
            "#,
        )
        .unwrap();
        let cam = cfg.sources[0].effective_params();
        assert_eq!(cam["uri"].as_str(), Some("rtmp://host/live/cam1"));
        assert_eq!(cam["client"].as_str(), Some("rtmpsrc"));
        let page = cfg.sources[1].effective_params();
        assert_eq!(page["superimpose"].as_str(), Some("auto"));
    }

    #[test]
    fn a_source_can_be_written_as_a_type_and_params_with_no_uri_field() {
        let cfg: Config = toml::from_str(
            r#"
            [[sources]]
            id = "bars"
            type = "test/source"
            params = { uri = "test://smpte" }
            "#,
        )
        .unwrap();
        let src = &cfg.sources[0];
        assert_eq!(src.type_id.as_deref(), Some("test/source"));
        assert_eq!(src.effective_params()["uri"].as_str(), Some("test://smpte"));
        cfg.validate().expect("test/source takes these params");
    }

    #[test]
    fn an_unknown_key_on_a_source_reaches_the_kind_rather_than_vanishing() {
        let cfg: Config = toml::from_str(
            r#"
            [[sources]]
            id = "cam1"
            uri = "rtmp://host/live/cam1"
            something_a_plugin_knows = "yes"
            "#,
        )
        .unwrap();
        assert_eq!(
            cfg.sources[0].effective_params()["something_a_plugin_knows"].as_str(),
            Some("yes")
        );
    }

    #[test]
    fn a_filter_has_to_say_where_it_goes() {
        let cfg: Config = toml::from_str(
            r#"
            [[filters]]
            id = "key"
            type = "chroma/filter"
            attach = { programme = true }
            params = { method = "green" }
            "#,
        )
        .unwrap();
        assert_eq!(cfg.filters[0].type_id, "chroma/filter");
        assert!(cfg.filters[0].attach.programme);
        cfg.validate().expect("a programme filter with good params is accepted");

        let nowhere: Config = toml::from_str(
            r#"
            [[filters]]
            id = "key"
            type = "chroma/filter"
            "#,
        )
        .unwrap();
        let err = nowhere.validate().expect_err("a filter with no attachment is refused");
        assert!(format!("{err:#}").contains("where it goes"), "{err:#}");
    }

    #[test]
    fn a_bad_param_names_the_field_and_what_it_takes() {
        let cfg: Config = toml::from_str(
            r#"
            [[filters]]
            id = "key"
            type = "chroma/filter"
            attach = { programme = true }
            params = { method = "puce" }
            "#,
        )
        .unwrap();
        let err = cfg.validate().expect_err("puce is not a keying method");
        let text = format!("{err:#}");
        assert!(text.contains("params.method"), "{text}");
        assert!(text.contains("green"), "{text}");
    }

    #[test]
    fn odd_canvas_is_rejected() {
        let mut cfg = Config {
            canvas: Canvas { width: 1921, height: 1080, fps: 30, sample_rate: 48000, channels: 2 },
            program: Default::default(),
            multiview: Default::default(),
            snapshot: Default::default(),
            control: Default::default(),
            hardware: Default::default(),
            codecs: Default::default(),
            media: Default::default(),
            security: Default::default(),
            safety: Default::default(),
            browser: Default::default(),
            stall: Default::default(),
            sources: vec![],
            outputs: vec![],
            filters: vec![],
            plugins: Default::default(),
            tokens: vec![],
            extra: Default::default(),
        };
        assert!(cfg.validate().is_err());
        cfg.canvas.width = 1920;
        assert!(cfg.validate().is_ok());
    }
}
