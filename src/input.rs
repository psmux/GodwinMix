//! One pipeline per source, fully isolated from the program pipeline.
//!
//! Isolation is the whole point of this module. If a camera's RTMP connection
//! drops, its decoder errors, or it starts sending garbage, the damage is
//! confined to that source's own `GstPipeline`. The program pipeline never sees
//! the bus error, never changes state, and the output encoder keeps running.
//!
//! The two pipelines are joined by `proxysink` / `proxysrc` pairs, which pass
//! buffers between pipelines without a socket or a copy.
//!
//! Every branch ends at a capsfilter carrying the canvas caps. Downstream of
//! that point every source in the system is byte-for-byte interchangeable,
//! which is what makes a take a property change rather than a renegotiation.

use crate::caps::CanvasCaps;
use crate::config::{BrowserConfig, SourceConfig, Superimpose};
use crate::gstutil::{self, make};
use crate::probe::Backends;
use crate::state::{SourceAudio, SourceHealth, SourceId, SourceState};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Thumbnails are produced at a fixed size in the input pipeline. The
/// multiview compositor scales each one to whatever its cell happens to be, so
/// adding or removing a source never rebuilds an input pipeline.
pub const THUMB_WIDTH: i32 = 480;
pub const THUMB_HEIGHT: i32 = 270;

/// Where a source's media comes from.
///
/// Both kinds end at the same capsfilter carrying the canvas caps, so once
/// normalised an ad file is indistinguishable from a camera as far as the
/// mixer is concerned. That is what lets an ad break reuse the take, the audio
/// crossfade and the slate behaviour without a second code path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// RTMP, demuxed and decoded explicitly so the client implementation can be
    /// chosen and swapped. See `RtmpClient`.
    Rtmp,
    /// Any other continuous stream: HLS, RTSP, SRT. Decoded by `uridecodebin`,
    /// which already knows how to handle each of them, and treated as live.
    Live,
    /// A rendered web page: whatever the browser draws and plays, captured as
    /// a live source. For a page whose media URL you can extract, adding that
    /// URL directly is far cheaper; this is for pages where the page itself is
    /// the content, such as a game or a dashboard.
    Web,
    /// A command line whose process writes media to stdout.
    ///
    /// The universal escape hatch: anything that can be produced by a program,
    /// in any language, becomes a source without a GStreamer element being
    /// written for it. The process writes a container GStreamer can open,
    /// MPEG-TS being the usual choice, and decoding then goes through the same
    /// hardware-aware path as every other source.
    Exec,
    /// A finite file. Ends with EOS, which is how an ad break knows to return.
    File,
}

/// The command behind an `exec:` source, if this is one.
pub fn exec_command(uri: &str) -> Option<&str> {
    let t = uri.trim();
    for prefix in ["exec://", "exec:"] {
        if t.len() >= prefix.len() && t[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return Some(t[prefix.len()..].trim());
        }
    }
    None
}

/// Prefixes that mark a URL as "render this page" rather than "open this
/// stream". An explicit marker is needed because `https://host/thing` is
/// genuinely ambiguous: it could be a page or a media file, and guessing wrong
/// means either a blank source or a browser started for nothing.
const WEB_PREFIXES: &[(&str, &str)] = &[
    ("web+https://", "https://"),
    ("web+http://", "http://"),
    // Bare `web://` is shorthand for https.
    ("web://", "https://"),
];

/// Mark a URL as a page to render, unless it already is one. What the API and
/// the CLI use when the operator has said "this is a website" instead of
/// typing the prefix.
pub fn as_web_uri(url: &str) -> String {
    let t = url.trim();
    if web_url(t).is_some() {
        return t.to_string();
    }
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        format!("web+{t}")
    } else {
        // A bare host or path: assume https.
        format!("web+https://{t}")
    }
}

/// The page URL behind a web source marker, if this is one.
pub fn web_url(uri: &str) -> Option<String> {
    let trimmed = uri.trim();
    let lower = trimmed.to_lowercase();
    WEB_PREFIXES.iter().find_map(|(prefix, scheme)| {
        lower
            .starts_with(prefix)
            .then(|| format!("{scheme}{}", &trimmed[prefix.len()..]))
    })
}

impl SourceKind {
    /// Work out how to open a URI.
    ///
    /// The distinction that matters is continuous versus finite, not the
    /// protocol: a continuous source is re-timed onto programme time and
    /// restarted when it drops, while a finite one is expected to end.
    pub fn detect(uri: &str) -> Self {
        if exec_command(uri).is_some() {
            return Self::Exec;
        }
        if web_url(uri).is_some() {
            return Self::Web;
        }
        let lower = uri.trim().to_lowercase();
        if lower.starts_with("rtmp://") || lower.starts_with("rtmps://") {
            return Self::Rtmp;
        }
        if lower.starts_with("rtsp://")
            || lower.starts_with("rtsps://")
            || lower.starts_with("srt://")
            || lower.starts_with("udp://")
            || lower.starts_with("rtp://")
        {
            return Self::Live;
        }
        // Playlist manifests are live regardless of being fetched over HTTP.
        let path = lower.split(['?', '#']).next().unwrap_or(&lower);
        if path.ends_with(".m3u8") || path.ends_with(".mpd") {
            return Self::Live;
        }
        // Anything else, including a plain file over HTTP, is finite.
        Self::File
    }

    /// True for sources expected to run indefinitely.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_continuous(self) -> bool {
        matches!(self, Self::Rtmp | Self::Live | Self::Web | Self::Exec)
    }

    /// Whether the stream should go through `livesync`.
    ///
    /// A network stream drifts against our clock and has gaps; livesync
    /// duplicates and drops to hand the canvas a steady, on-time feed. A
    /// process writing to a pipe is different: its timestamps count from
    /// its own start, and the browser sidecar starts stamping half a second
    /// after the pipeline that reads it. livesync judged every one of those
    /// frames late against our clock, dropped the lot, and repeated the
    /// first black frame forever while the audio (not synced) played on.
    /// The process already paces its output; the mixer pad offset places it
    /// and the compositor's latency absorbs the start-up gap.
    pub fn wants_livesync(self) -> bool {
        matches!(self, Self::Rtmp | Self::Live | Self::Web)
    }
}

/// Turn a bare path into a URI. `uridecodebin` needs a real URI, but an
/// operator scheduling an ad will reasonably type a path.
pub fn to_uri(input: &str) -> String {
    if input.contains("://") {
        return input.to_string();
    }
    let path = std::path::Path::new(input);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    file_uri(&abs)
}

/// A `file:` URI for a path, spelled the way GLib spells it on this platform.
/// `format!("file://{}")` gives `file://C:\clips\a.mp4` on Windows, which no
/// element opens; GLib percent-encodes and puts the drive where a URI wants it.
pub fn file_uri(path: &std::path::Path) -> String {
    gst::glib::filename_to_uri(path, None)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.display()))
}

pub struct InputPipeline {
    pub id: SourceId,
    pub config: SourceConfig,
    pub pipeline: gst::Pipeline,
    pub health: Arc<SourceHealth>,
    /// Proxy sinks the program and multiview pipelines attach to. They are
    /// created up front, before any media has arrived, so that the mixer can
    /// allocate its pads without waiting for a camera to connect.
    pub video_proxy: gst::Element,
    pub thumb_proxy: gst::Element,
    pub audio_proxy: gst::Element,
    has_video: Arc<AtomicBool>,
    has_audio: Arc<AtomicBool>,
    /// Whether the page's media ended up being decoded outside the browser.
    /// Settled when the pipeline is built and constant for its lifetime.
    superimposed: bool,
    /// Present only for a superimposed source, which is the only kind whose
    /// sounds arrive separately enough to be balanced. See `AudioLevels`.
    levels: Option<AudioLevels>,
    /// Where a layered source's layers sit in time, one per layer. Reset on
    /// restart.
    placement: Vec<Arc<Placement>>,
    /// Local copies of the page's videos, when made. See `cache_media`.
    media_cache: Vec<std::path::PathBuf>,
    /// Set when the pipeline posts an error; the supervisor restarts it.
    failed: Arc<AtomicBool>,
    /// Whether this pipeline can be scrubbed, once it has said. `None` until
    /// then, because nothing upstream answers a SEEKING query before the chain
    /// from the source to the proxies is built, and a query nobody answered is
    /// not the same as a no.
    seekable: Mutex<Option<bool>>,
    /// The RTMP client element, swappable once if the configured one turns out
    /// not to talk to this server. See `RtmpClient`.
    source: Mutex<gst::Element>,
    /// Present only for RTMP sources, where the client element is swappable.
    src_queue: Option<gst::Element>,
    fallback_used: AtomicBool,
    /// Set while a restart is pending. A source whose server has gone away
    /// emits a burst of bus errors, and without this each one arms its own
    /// restart. They then all fire together, tearing the pipeline down and
    /// rebuilding it dozens of times in a few milliseconds, which is enough to
    /// take the whole process down.
    restart_armed: AtomicBool,
    /// The child process behind an `exec:` source. Killed when the source is
    /// stopped or restarted, so a rebuilt pipeline never leaves an orphan
    /// writing into a pipe nobody reads.
    exec_child: Mutex<Option<ExecChild>>,
    /// How to start (and restart) the process behind an exec source.
    exec: Option<ExecSpec>,
    #[allow(dead_code)]
    kind: SourceKind,
}

/// A command line to run as a source: the program and its arguments, already
/// split, plus environment on top of ours. Built either from an `exec:` URI
/// (after the security check) or by the mixer itself for a browser page.
#[derive(Debug, Clone)]
pub struct ExecSpec {
    pub argv: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
}

impl ExecSpec {
    /// The command behind an `exec:` URI, if exec sources are allowed.
    pub fn from_uri(uri: &str, allowed: bool) -> Result<Self> {
        let command = exec_command(uri).context("not an exec source")?;
        anyhow::ensure!(
            allowed,
            "exec sources are disabled. They run a command line on this machine, so \
             anyone who can reach the control port could run anything. Set \
             security.allow_exec_sources = true only if that port is on a trusted network."
        );
        anyhow::ensure!(!command.is_empty(), "exec source has an empty command");
        let argv = shell_words::split(command)
            .with_context(|| format!("parsing command: {command}"))?;
        anyhow::ensure!(!argv.is_empty(), "exec source has an empty command");
        Ok(Self { argv, env: Default::default() })
    }

    /// The CEF sidecar rendering `uri`'s page at the canvas size, or `None`
    /// when no sidecar is installed and `wpesrc` should be tried instead.
    ///
    /// This is not gated by `allow_exec_sources`: the program is ours and the
    /// page URL travels as one argument, never through a shell.
    pub fn browser(uri: &str, canvas: &CanvasCaps, browser: &BrowserConfig) -> Result<Option<Self>> {
        let url = web_url(uri).context("not a web source url")?;
        let Some(sidecar) = find_browser_sidecar(browser)? else {
            return Ok(None);
        };
        let mut argv = vec![
            sidecar.to_string_lossy().to_string(),
            "--url".into(),
            url,
            "--width".into(),
            canvas.width.to_string(),
            "--height".into(),
            canvas.height.to_string(),
            "--fps".into(),
            canvas.fps.numer().to_string(),
        ];
        argv.extend(browser.args.iter().cloned());
        Ok(Some(Self { argv, env: browser.env.clone() }))
    }

    /// Rewrite the `--fps` this spec was built with.
    ///
    /// Edited in place rather than appended. The sidecar's parser takes the
    /// last spelling of a flag, and `browser.args` is appended after this, so
    /// appending here would quietly outrank an explicit `--fps` the operator
    /// put in their own config. Editing the one we built leaves theirs winning,
    /// which is the way round it should be.
    fn set_fps(&mut self, fps: u32) {
        if let Some(i) = self.argv.iter().position(|a| a == "--fps") {
            if let Some(v) = self.argv.get_mut(i + 1) {
                *v = fps.to_string();
            }
        }
    }
}

/// Where `liveboxmix-browser` is: the configured path, else next to this
/// executable, else on PATH.
fn find_browser_sidecar(browser: &BrowserConfig) -> Result<Option<std::path::PathBuf>> {
    const NAME: &str = "liveboxmix-browser";
    if let Some(p) = &browser.sidecar {
        let p = std::path::PathBuf::from(p);
        anyhow::ensure!(
            p.is_file(),
            "browser.sidecar is set to {} but there is no such file",
            p.display()
        );
        return Ok(Some(p));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())) {
        // On macOS CEF only runs from an app bundle, so that is what sits
        // next to the mixer there.
        let candidates = [
            dir.join(format!("{NAME}{}", std::env::consts::EXE_SUFFIX)),
            dir.join(format!("{NAME}.app")).join("Contents/MacOS").join(NAME),
        ];
        if let Some(p) = candidates.into_iter().find(|p| p.is_file()) {
            return Ok(Some(p));
        }
    }
    let found = std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .map(|d| d.join(format!("{NAME}{}", std::env::consts::EXE_SUFFIX)))
                .find(|p| p.is_file())
        })
        .unwrap_or(None);
    Ok(found)
}

/// What the sidecar found the page playing.
///
/// The fields the mixer acts on, out of the report `--detect-media` writes on
/// stderr. Everything else in that report is diagnostic and is ignored, which
/// is also why unknown fields must not be an error: the sidecar is free to say
/// more without this failing to parse.
#[derive(Debug, Clone, Deserialize)]
pub struct MediaReport {
    /// Whether the page had a media element at all when this was written.
    #[serde(default)]
    found: bool,
    /// The headline element, the one a viewer would call "the video". These
    /// fields are what a sidecar from before `media` existed reports, and they
    /// are still filled in for it.
    #[serde(default)]
    src: String,
    #[serde(default)]
    usable: bool,
    #[serde(default)]
    mse: bool,
    #[serde(default)]
    drm: bool,
    #[serde(default)]
    rect: MediaRect,
    #[serde(default)]
    viewport: MediaSize,
    /// Every video on the page, in document order. After the probe has run
    /// this holds only the ones the mixer will draw itself.
    #[serde(default)]
    media: Vec<MediaItem>,
}

/// One video element on the page.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MediaItem {
    /// Position in document order, which is also its place in the stacking.
    #[serde(default)]
    index: usize,
    /// The address a decoder outside the browser can open.
    #[serde(default)]
    src: String,
    /// The sidecar's own verdict on that address. False for a `blob:` URL fed
    /// by JavaScript, for DRM, and for an element with no source yet.
    #[serde(default)]
    usable: bool,
    /// The element is fed from JavaScript through Media Source Extensions, so
    /// its address is a `blob:` that exists only inside that renderer.
    #[serde(default)]
    mse: bool,
    /// Encrypted Media Extensions: decrypted inside the browser, never leaves.
    #[serde(default)]
    drm: bool,
    /// The page plays this one silently. The mixer's copy is muted to match.
    #[serde(default)]
    muted: bool,
    /// Where the element sat in the page, in CSS pixels.
    #[serde(default)]
    rect: MediaRect,
    /// The size of the viewport that rectangle was measured in.
    #[serde(default)]
    viewport: MediaSize,
    /// A local copy of a finite clip, when one was fetched. See `cache_media`.
    /// Deleted when the source stops.
    #[serde(skip)]
    cache: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
struct MediaRect {
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default)]
    w: i32,
    #[serde(default)]
    h: i32,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
struct MediaSize {
    #[serde(default)]
    w: i32,
    #[serde(default)]
    h: i32,
}

impl MediaReport {
    /// Whether anything on the page can be handed over.
    fn any_usable(&self) -> bool {
        self.usable || self.media.iter().any(|m| m.usable)
    }

    /// The videos in the report, as items. A report from a sidecar that only
    /// knew about one video has an empty `media` list and its headline fields
    /// carry that one, so it becomes the single item.
    fn items(&self) -> Vec<MediaItem> {
        if !self.media.is_empty() {
            return self.media.clone();
        }
        if !self.usable {
            return Vec::new();
        }
        vec![MediaItem {
            index: 0,
            src: self.src.clone(),
            usable: true,
            mse: self.mse,
            drm: self.drm,
            muted: false,
            rect: self.rect,
            viewport: self.viewport,
            cache: None,
        }]
    }

    /// Where the headline video goes on the canvas. See `MediaItem::placement`.
    #[cfg(test)]
    fn placement(&self, canvas: &CanvasCaps) -> (i32, i32, i32, i32) {
        placement_of(self.rect, self.viewport, canvas)
    }
}

impl MediaItem {
    /// Where this video goes on the canvas.
    fn placement(&self, canvas: &CanvasCaps) -> (i32, i32, i32, i32) {
        placement_of(self.rect, self.viewport, canvas)
    }
}

/// The page measured its video in viewport pixels and the canvas may be a
/// different size, so the rectangle is scaled by the ratio between them. A
/// video filling its viewport therefore fills the canvas, and one sitting in a
/// corner of the page stays in that corner.
///
/// A rectangle that makes no sense, which is what a report from a page
/// mid-layout looks like, falls back to the whole canvas.
fn placement_of(rect: MediaRect, viewport: MediaSize, canvas: &CanvasCaps) -> (i32, i32, i32, i32) {
    let full = (0, 0, canvas.width, canvas.height);
    if rect.w <= 0 || rect.h <= 0 || viewport.w <= 0 || viewport.h <= 0 {
        return full;
    }
    let sx = f64::from(canvas.width) / f64::from(viewport.w);
    let sy = f64::from(canvas.height) / f64::from(viewport.h);
    let scale = |v: i32, s: f64| (f64::from(v) * s).round() as i32;
    let (w, h) = (scale(rect.w, sx), scale(rect.h, sy));
    if w <= 0 || h <= 0 {
        return full;
    }
    (scale(rect.x, sx), scale(rect.y, sy), w, h)
}

/// The prefix the sidecar puts on every media report.
const MEDIA_LINE: &str = "[browser] media ";

/// How long to give the sidecar to say what the page is playing.
///
/// Long enough that a page has loaded, autoplay has started and the first
/// report has been written. That took about three seconds against the local
/// test pages from a native sidecar, six from one in a container, and eleven
/// on a machine under load, where a ten second ceiling quietly gave up and the
/// source rendered the page whole without anyone being told why. Nothing is
/// lost by waiting: the probe runs on its own thread before the pipeline
/// exists, so what it costs is how long the operator waits for the source to
/// appear, not a gap in the programme, and a page whose media can never be
/// handed over ends the wait as soon as it says so.
pub const MEDIA_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// How far a page frame may be from the moment it arrives before the page
/// is stamped afresh. Frames within the window go out as they are.
const PAGE_DRIFT_NS: u64 = 150_000_000;

/// How far ahead of the clock a video and its sound are placed on arrival.
///
/// The audio mixer discards a sample that lands even slightly behind its
/// output position, and then every one after it is behind by the same amount.
/// A small margin ahead costs that much delay and keeps the sound; the picture
/// takes the same margin so the two stay together.
const MEDIA_LEAD_NS: u64 = 300_000_000;

/// How far behind the clock the next round of a media may be placed and still
/// be joined to the round before it.
///
/// A round joins where the one before it ended, and that moment has usually
/// just gone by: the new round cannot start until the queues holding the old
/// one have drained. The picture gets through within a few milliseconds of its
/// join; the sound, whose branch is the last of the two to let a new segment
/// past, measured half a second behind it on the rig.
///
/// Placing it at the join anyway is right, because the layer aggregators
/// compose `LAYER_LATENCY_NS` behind the clock and trim exactly the part of a
/// round that is behind them. A round half a second late loses the few
/// milliseconds that are genuinely past and plays the rest, where placing it
/// at `now` instead was half a second of silence and placing it at `now` plus
/// the lead was three quarters of a second. The ceiling is only there so that
/// a round that has lost seconds, rather than milliseconds, is not pushed
/// through an aggregator that will throw all of it away.
const MEDIA_JOIN_SLACK_NS: u64 = 2_000_000_000;

/// Upstream latency the layered compositor claims, the same figure as the
/// mixer's own compositor. It is how late a page frame may be before it is
/// dropped, and how long the media's next round may take to arrive before the
/// gap shows on air.
const LAYER_LATENCY_NS: i64 = 500_000_000;

/// The report line's payload, if this is one.
fn media_report(line: &str) -> Option<MediaReport> {
    let (_, json) = line.split_once(MEDIA_LINE)?;
    serde_json::from_str(json).ok()
}

/// Run the sidecar once, purely to find out whether the page's video has an
/// address we can open ourselves.
///
/// This is a throwaway render: the page is loaded, given a few seconds to start
/// playing, and the process is killed as soon as it has reported. Deciding
/// before anything is built is what keeps the pipeline shape fixed for the
/// source's lifetime. The alternative, starting the ordinary web source and
/// rebuilding it around a compositor once a report arrives, means relinking a
/// running pipeline in front of the programme output, and there is no
/// version of that which is worth the seconds it saves.
///
/// Its stdout goes to /dev/null on purpose. The sidecar writes raw video there
/// at tens of megabytes a second, and a pipe nobody drains would block it
/// before it ever loaded the page.
pub fn probe_page_media(id: &SourceId, spec: &ExecSpec, timeout: Duration) -> Option<MediaReport> {
    let mut spec = spec.clone();
    spec.argv.push("--detect-media".into());
    // A backstop, so a sidecar that somehow outlives the kill below still ends.
    spec.argv.push("--seconds".into());
    spec.argv.push(timeout.as_secs().max(1).to_string());

    let mut child = match exec_process(&spec, std::process::Stdio::null()) {
        Ok(c) => c,
        Err(e) => {
            warn!(source = %id, ?e, "could not probe the page for media; rendering it whole");
            return None;
        }
    };
    let Some(err) = child.stderr.take() else {
        warn!(source = %id, "probe child has no stderr; rendering the page whole");
        stop_child(child.id(), &spec.env);
        #[cfg(not(unix))]
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };

    // The read happens on its own thread because a pipe read cannot be given a
    // deadline. `StderrReader` is what makes that thread and its descriptor go
    // away for certain: the kill below closes the pipe only if nothing the
    // sidecar started is still holding the write end, and on air on
    // 2026-09-12 plenty were.
    let (tx, rx) = std::sync::mpsc::channel();
    let mut reader = StderrReader::spawn(format!("media-probe-{id}"), err, move |line| {
        if let Some(report) = media_report(line) {
            let _ = tx.send(report);
        }
    });

    let started = Instant::now();
    let deadline = started + timeout;
    let mut found = None;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match rx.recv_timeout(left) {
            Ok(report) if report.any_usable() => {
                found = Some(report);
                break;
            }
            // The players are there and none of what they have can be handed
            // over. No point waiting out the clock for that to change.
            Ok(report)
                if report.found
                    && !report.media.is_empty()
                    && report.media.iter().all(|m| m.mse || m.drm) =>
            {
                info!(
                    source = %id,
                    mse = report.mse,
                    drm = report.drm,
                    "the page's video cannot be handed over; rendering the page whole"
                );
                break;
            }
            // A page reports as it loads, and the first report is often from
            // before the player has a source. Keep listening until the timeout.
            Ok(_) => continue,
            Err(_) => break,
        }
    }

    stop_child(child.id(), &spec.env);
    #[cfg(not(unix))]
    let _ = child.kill();
    let _ = child.wait();
    reader.stop();
    // Fetch each clip once, and keep only what can actually be played. A video
    // whose address turns out to be dead (a 404 was the case that found this)
    // is left to the browser rather than built into a layer that fails and
    // takes the whole source down with it.
    if let Some(r) = found.as_mut() {
        let mut items = r.items();
        items.retain(|m| m.usable);
        for m in items.iter_mut() {
            match cache_media(id, &mut m.src) {
                Fetched::Copy(path) => m.cache = Some(path),
                Fetched::Stream => {}
                Fetched::Failed => m.usable = false,
            }
        }
        items.retain(|m| m.usable);
        if items.is_empty() {
            found = None;
        } else {
            r.media = items;
        }
    }
    match &found {
        Some(r) => info!(
            source = %id,
            videos = r.media.len(),
            first = %r.media[0].src,
            secs = started.elapsed().as_secs_f64(),
            "the page's videos have addresses we can open"
        ),
        None => info!(
            source = %id,
            secs = started.elapsed().as_secs_f64(),
            "no usable media on this page; rendering it whole"
        ),
    }
    found
}

/// How far ahead of the picture the page's media is decoded, in seconds. See
/// the media queue in `Layers::build`.
const MEDIA_LEAD_SECS: f64 = 4.0;

/// How long the loop waits for the sound of a round to finish once the picture
/// has, before starting the decoder again anyway. Both have a queue of
/// `MEDIA_LEAD_SECS`, so in practice this is milliseconds; the ceiling only
/// stops a stream that never ends from stopping the loop for good.
const MEDIA_END_WAIT: Duration = Duration::from_secs(2);

/// How long a clip may take to fetch before it is streamed instead.
const MEDIA_FETCH_TIMEOUT: Duration = Duration::from_secs(60);

/// Fetch a finite clip to a local file once, and point `src` at the copy.
///
/// The mixer loops the page's video itself, and looping over the network costs
/// a fresh connection, an index read and a decoder start every time round.
/// Measured on the rig that was 2.6 to 4.6 seconds of frozen picture at each
/// loop, and it was worse the busier the machine. From a local file the same
/// switch is tens of milliseconds, inside the latency the compositor already
/// claims, so the join does not show. It also stops the loop depending on the
/// server: one that will not honour a range request cannot be seeked back to
/// the start, and a Python test server is exactly such a one.
///
/// Only for what looks like a clip. An HLS or DASH address is a stream, plays
/// directly and never loops. Anything that has not finished arriving after
/// `MEDIA_FETCH_TIMEOUT` is treated as a stream too and played from the
/// address; the partial file is removed.
/// What `cache_media` did with an address.
enum Fetched {
    /// A stream: nothing to fetch, play it from the address.
    Stream,
    /// A clip, now on disk, and `src` points at the copy.
    Copy(std::path::PathBuf),
    /// Neither: the address could not be read at all.
    Failed,
}

fn cache_media(id: &SourceId, src: &mut String) -> Fetched {
    let path_part = src.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
    if path_part.ends_with(".m3u8") || path_part.ends_with(".mpd") {
        debug!(source = %id, "the page's video is a stream; playing it from its address");
        return Fetched::Stream;
    }
    let Some(factory) = ["souphttpsrc", "curlhttpsrc"]
        .into_iter()
        .find(|f| crate::probe::exists(f))
    else {
        return Fetched::Stream;
    };
    let stem = path_part.rsplit('/').next().unwrap_or("clip").replace(|c: char| !c.is_ascii_alphanumeric() && c != '.', "_");
    // Numbered per fetch, not only per process: a source rebuilt after its
    // browser died fetches its clips again while the old pipeline, torn down
    // on another thread, is deleting its own, and with the same names the new
    // copy would go with the old.
    static FETCHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = FETCHES.fetch_add(1, Ordering::SeqCst);
    let file = std::env::temp_dir().join(format!("lbx-media-{id}-{}-{n}-{stem}", std::process::id()));
    let fetch = || -> Result<()> {
        let pipeline = gst::Pipeline::with_name(&format!("fetch-{id}"));
        let http = make(factory, &format!("{id}-fetch-src"))?;
        http.set_property("location", &*src);
        let sink = make("filesink", &format!("{id}-fetch-sink"))?;
        sink.set_property("location", file.to_string_lossy().as_ref());
        pipeline.add_many([&http, &sink]).context("adding fetch elements")?;
        http.link(&sink).context("linking fetch")?;
        pipeline.set_state(gst::State::Playing).context("starting fetch")?;
        let bus = pipeline.bus().context("fetch pipeline has no bus")?;
        let msg = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(MEDIA_FETCH_TIMEOUT.as_secs()),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
        let _ = pipeline.set_state(gst::State::Null);
        match msg.as_ref().map(|m| m.view()) {
            Some(gst::MessageView::Eos(_)) => Ok(()),
            Some(gst::MessageView::Error(e)) => anyhow::bail!("{}", e.error()),
            _ => anyhow::bail!("not finished after {}s", MEDIA_FETCH_TIMEOUT.as_secs()),
        }
    };
    let started = Instant::now();
    match fetch() {
        Ok(()) => {
            let bytes = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
            info!(
                source = %id,
                bytes,
                secs = started.elapsed().as_secs_f64(),
                "fetched the page's video once, to loop it from disk"
            );
            *src = file_uri(&file);
            Fetched::Copy(file)
        }
        Err(e) => {
            warn!(source = %id, src = %src, ?e, "could not fetch the page's video; leaving it to the browser");
            let _ = std::fs::remove_file(&file);
            Fetched::Failed
        }
    }
}

/// The elements that exist only on the layered path.
///
/// The page's video decoded here as the bottom layer, the page itself drawn
/// over the top, and a compositor joining them. Everything downstream of
/// `comp_caps` is the ordinary normalising chain, so the mixer above cannot
/// tell a superimposed source from any other one.
struct Layers {
    media: Vec<MediaBranch>,
    over_q: gst::Element,
    over_conv: gst::Element,
    /// The page's own sound: whatever is still playing in the browser once
    /// the taken-over videos are muted, mixed in with theirs.
    page_aconv: gst::Element,
    page_ares: gst::Element,
    /// Level for the page's own sound, so commentary and the quiz's effects
    /// can be balanced against a video the mixer is playing underneath. The
    /// media branches have had their own `volume` all along; this is the other
    /// half of that, and the two together are what makes the balance settable.
    page_vol: gst::Element,
    amix: gst::Element,
    comp: gst::Element,
    comp_caps: gst::Element,
    flat_conv: gst::Element,
    flat_caps: gst::Element,
}

/// One video the mixer draws itself: its decoder, the picture path into the
/// compositor and the sound path into the mix.
struct MediaBranch {
    item: MediaItem,
    src: gst::Element,
    conv: gst::Element,
    caps: gst::Element,
    q: gst::Element,
    scale: gst::Element,
    /// The sound's own lead, the same length as the picture's queue above.
    /// Without it the decoder could only push sound as fast as the live mix
    /// took it, so the sound ran `MEDIA_LEAD_SECS` behind the picture inside
    /// the decoder: at the end of a round the picture had reached the end of
    /// the clip while the sound still had four seconds to push, and starting
    /// the decoder again threw those four seconds away. Every loop opened with
    /// that much silence. With a queue of its own the sound runs ahead exactly
    /// as the picture does and is already decoded when the round changes.
    aq: gst::Element,
    aconv: gst::Element,
    ares: gst::Element,
    /// Muted when the page played this video silently, so the mix matches
    /// what a viewer of the page would hear.
    avol: gst::Element,
}

impl MediaBranch {
    fn build(id: &SourceId, item: MediaItem) -> Result<Self> {
        let n = item.index;
        let src = make("uridecodebin", &format!("{id}-src-media{n}"))?;
        src.set_property("uri", &item.src);
        // No use-buffering here, unlike a plain media source. It posts buffering
        // messages that put the pipeline into PAUSED, and pausing this pipeline
        // would also stop the page, which is live and cannot be paused. The
        // queue below is what absorbs a slow server instead.
        //
        // Not `uridecodebin3` with its gapless `about-to-finish`, which was
        // tried: on the rig it took ten seconds to move to the next item even
        // from a local file, for reasons that did not show in the logs. The
        // loop is done in `Placement` instead, by restarting this element from
        // the local copy `cache_media` made, which is tens of milliseconds and
        // entirely in view.
        let avol = make("volume", &format!("{id}-media{n}-vol"))?;
        avol.set_property("mute", item.muted);
        Ok(Self {
            src,
            // Before the queue, not after it, and forced to a real conversion.
            // A hardware decoder hands out frames from a small pool of its own
            // surfaces and stops decoding when they are all held, so a queue
            // full of its frames holds a handful, whatever its time limit says.
            // Copying each frame into ordinary memory here gives the surface
            // back at once, and the queue then holds what it was asked to.
            conv: make("videoconvert", &format!("{id}-media{n}-conv"))?,
            caps: gstutil::capsfilter(
                &format!("{id}-media{n}-caps"),
                &gst::Caps::builder("video/x-raw").field("format", "I420").build(),
            )?,
            // The media decodes far faster than real time and then waits on
            // the compositor, and this queue is where those frames sit. Its
            // length is the lead the loop gets: the end of stream passes the
            // decoder's pad when the last frame enters the queue, so the
            // decoder is started again that long before the picture runs out,
            // and it measured three seconds from restart to first frame on a
            // loaded machine. Four seconds of decoded frames is 170 MB at 720p
            // per video, held only while the source exists.
            q: gstutil::queue_time(&format!("{id}-media{n}-q"), MEDIA_LEAD_SECS, false)?,
            scale: make("videoscale", &format!("{id}-media{n}-scale"))?,
            aq: gstutil::queue_time(&format!("{id}-media{n}-aq"), MEDIA_LEAD_SECS, false)?,
            aconv: make("audioconvert", &format!("{id}-media{n}-aconv"))?,
            ares: make("audioresample", &format!("{id}-media{n}-ares"))?,
            avol,
            item,
        })
    }

    fn elements(&self) -> [&gst::Element; 9] {
        [
            &self.src, &self.conv, &self.caps, &self.q, &self.scale, &self.aq, &self.aconv,
            &self.ares, &self.avol,
        ]
    }
}

impl Layers {
    /// The level elements, for the control API. Cloning a gst Element clones
    /// the handle, not the element, so these stay the ones in the pipeline.
    fn levels(&self) -> AudioLevels {
        AudioLevels {
            page: self.page_vol.clone(),
            media: self.media.iter().map(|b| b.avol.clone()).collect(),
        }
    }

    fn build(id: &SourceId, report: &MediaReport) -> Result<Self> {
        let media = report
            .media
            .iter()
            .cloned()
            .map(|item| MediaBranch::build(id, item))
            .collect::<Result<Vec<_>>>()?;
        anyhow::ensure!(!media.is_empty(), "a layered source needs at least one video");
        Ok(Self {
            media,
            over_q: gstutil::queue_thread(&format!("{id}-over-q"))?,
            over_conv: make("videoconvert", &format!("{id}-over-conv"))?,
            page_aconv: make("audioconvert", &format!("{id}-page-aconv"))?,
            page_ares: make("audioresample", &format!("{id}-page-ares"))?,
            page_vol: make("volume", &format!("{id}-page-vol"))?,
            // Live like the compositor below, for the same reason: a sound
            // that stops must not stop the rest.
            amix: gstutil::make_live_aggregator("audiomixer", &format!("{id}-sup-amix"))?,
            comp: gstutil::make_live_aggregator("compositor", &format!("{id}-sup-comp"))?,
            // The compositor must be asked for a format that has alpha, and
            // this is the one thing here that is not obvious. `compositor`
            // converts every input to its *output* format before blending, so
            // an I420 output converts the page's AYUV first and the alpha is
            // gone by the time the blend happens: the page comes out opaque and
            // hides the video completely. Measured on the programme with an
            // I420 output, the half transparent white bar read 255 instead of
            // the 135 it should be over black, and the video never appeared.
            // AYUV costs a 4:4:4 blend and one conversion back, and is what
            // makes the feature work at all.
            comp_caps: gstutil::capsfilter(
                &format!("{id}-sup-caps"),
                &gst::Caps::builder("video/x-raw").field("format", "AYUV").build(),
            )?,
            // And back to I420 immediately, so what leaves this bin is what
            // every other source produces and nothing downstream has to know
            // the page ever had transparency.
            flat_conv: make("videoconvert", &format!("{id}-sup-flat-conv"))?,
            flat_caps: gstutil::capsfilter(
                &format!("{id}-sup-flat-caps"),
                &gst::Caps::builder("video/x-raw").field("format", "I420").build(),
            )?,
        })
    }

    fn elements(&self) -> Vec<&gst::Element> {
        let mut all: Vec<&gst::Element> = self.media.iter().flat_map(|b| b.elements()).collect();
        all.extend([
            &self.over_q,
            &self.over_conv,
            &self.page_aconv,
            &self.page_ares,
            &self.page_vol,
            &self.amix,
            &self.comp,
            &self.comp_caps,
            &self.flat_conv,
            &self.flat_caps,
        ]);
        all
    }

    /// Link every layer into the compositor and the compositor into `vrate`,
    /// the head of the normalising chain every source shares; and every sound
    /// into the mix and the mix into `audio_entry`. Returns the compositor
    /// pads, one per video in order and then the page's, for `place_in_time`.
    ///
    /// The compositor is a live aggregator, built the same way as the mixer's
    /// own and for the same reason. It times its output against the pipeline
    /// clock, so it produces frames as soon as the first layer has one and
    /// keeps producing them when a layer stops: a media branch that ends, or a
    /// server that goes quiet, leaves the page over whatever was last drawn
    /// instead of freezing the whole source until the supervisor rebuilds it.
    /// An earlier version composed on timestamps instead and did exactly that,
    /// every fifty seconds, on a twenty second clip.
    ///
    /// A live aggregator discards what arrives late, and the page's frames
    /// would all be late on their own: the sidecar stamps them from its own
    /// start, and its start is seconds before its first frame reaches us.
    /// `Placement` moves each layer's timeline to where its frames actually
    /// turned up, which is what makes them agree.
    fn link(
        &self,
        canvas: &CanvasCaps,
        vrate: &gst::Element,
        audio_entry: &gst::Element,
    ) -> Result<(Vec<gst::Pad>, gst::Pad)> {
        // Anywhere no layer covers is black, not the checkerboard the element
        // defaults to.
        self.comp.set_property_from_str("background", "black");
        // Not `ignore-inactive-pads`, which the mixer's own compositor uses.
        // Here a pad can go a long time before its first buffer: the page's
        // arrives fifteen seconds after the videos', once its browser has
        // loaded. With that property on, the compositor queued the page's
        // frames and never looked at the pad again, and the page never
        // appeared. Without it a live aggregator waits its latency for a pad
        // with nothing yet and carries on, which is the behaviour wanted.
        for agg in [&self.comp, &self.amix] {
            crate::probe::set_int(agg, "min-upstream-latency", LAYER_LATENCY_NS);
            // Begin the output timeline at the first buffer, not at running
            // time zero. This pipeline runs on the programme's clock and base
            // time (see the mixer, where a source is started), so its running
            // time is the programme's age; left at the default, a source added
            // five minutes into a broadcast had a compositor that started five
            // minutes in the past and composited black at whatever rate the
            // machine allowed until it caught up, which on a busy one it never
            // did. The page's frames, stamped at their arrival, sat that far in
            // its future and were never shown.
            agg.set_property_from_str("start-time-selection", "first");
        }
        // The page draws at a tenth of the canvas rate and the compositor
        // would happily pick that as its output rate. Pin it, so the videos
        // underneath keep every frame they have.
        self.comp_caps.set_property(
            "caps",
            &gst::Caps::builder("video/x-raw")
                .field("format", "AYUV")
                .field("framerate", canvas.fps)
                .build(),
        );

        let mut media_pads = Vec::with_capacity(self.media.len());
        for (z, b) in self.media.iter().enumerate() {
            gst::Element::link_many([&b.conv, &b.caps, &b.q, &b.scale])
                .context("linking a decoded video branch")?;
            gst::Element::link_many([&b.aq, &b.aconv, &b.ares, &b.avol, &self.amix])
                .context("linking a video's sound into the mix")?;
            let (x, y, w, h) = b.item.placement(canvas);
            // A pixel of bleed on every side, within the canvas. The page keys
            // out its video's box to the pixel and the decoded picture lands on
            // a rounded rectangle; where the two disagree by one, the
            // compositor's background would show as a line.
            let (x, y) = ((x - 1).max(0), (y - 1).max(0));
            let (w, h) = ((w + 2).min(canvas.width - x), (h + 2).min(canvas.height - y));
            let pad = self
                .comp
                .request_pad_simple("sink_%u")
                .context("compositor refused a pad for a video")?;
            // Stacked in document order, all of them under the page.
            pad.set_property("zorder", z as u32);
            pad.set_property("xpos", x);
            pad.set_property("ypos", y);
            pad.set_property("width", w);
            pad.set_property("height", h);
            // Letterbox inside the rectangle the page gave the video rather
            // than stretching it, the same choice the mixer makes for a source.
            pad.set_property_from_str("sizing-policy", "keep-aspect-ratio");
            // Set explicitly rather than trusted: the mixer never sets this
            // property anywhere else, so nothing here has ever depended on
            // the default being `over`.
            pad.set_property_from_str("operator", "over");
            b.scale
                .static_pad("src")
                .context("video branch has no src pad")?
                .link(&pad)
                .context("linking a video into the compositor")?;
            info!(video = b.item.index, x, y, width = w, height = h, muted = b.item.muted, "page video placed on the canvas");
            media_pads.push(pad);
        }

        // The page arrives from the sidecar already keyed: alpha zero where a
        // video the mixer draws itself used to be. See `KEY_TOLERANCE` in the
        // sidecar's mux.rs for why that happens there and not here.
        gst::Element::link_many([&self.over_q, &self.over_conv]).context("linking the page branch")?;
        gst::Element::link_many([&self.page_aconv, &self.page_ares, &self.page_vol, &self.amix])
            .context("linking the page's sound into the mix")?;
        let over_pad = self
            .comp
            .request_pad_simple("sink_%u")
            .context("compositor refused a pad for the page")?;
        over_pad.set_property("zorder", self.media.len() as u32);
        over_pad.set_property("xpos", 0i32);
        over_pad.set_property("ypos", 0i32);
        over_pad.set_property("width", canvas.width);
        over_pad.set_property("height", canvas.height);
        over_pad.set_property_from_str("sizing-policy", "keep-aspect-ratio");
        // The page carries a real alpha channel and this is what makes the
        // compositor honour it. `source` would paint the transparent parts of
        // the page over the videos as black.
        over_pad.set_property_from_str("operator", "over");
        self.over_conv
            .static_pad("src")
            .context("page branch has no src pad")?
            .link(&over_pad)
            .context("linking the page into the compositor")?;

        gst::Element::link_many([&self.comp, &self.comp_caps, &self.flat_conv, &self.flat_caps, vrate])
            .context("linking the composed layers into the normaliser")?;
        self.amix.link(audio_entry).context("linking the mix into the audio chain")?;
        Ok((media_pads, over_pad))
    }

    /// Put every layer on the composite's timeline and keep it there. See
    /// `Placement`. One placement per video, since each loops on its own, and
    /// one for the page.
    fn place_in_time(&self, id: &SourceId, page_src: &gst::Element) -> Result<Vec<Arc<Placement>>> {
        let mut all = Vec::new();
        let page = Placement::new();
        // The page's picture and sound each arrive on a pad of the sidecar's
        // decoder once its stream has been demuxed, and are watched there.
        let (me, pid) = (page.clone(), id.clone());
        page_src.connect_pad_added(move |_el, pad| {
            let media = pad
                .current_caps()
                .and_then(|c| c.structure(0).map(|s| s.name().to_string()))
                .unwrap_or_default();
            let name = pad.name();
            if name.starts_with("video") || media.starts_with("video/") || name.starts_with("audio") || media.starts_with("audio/") {
                me.watch(&pid, Stream::Page, pad, None);
            }
        });
        all.push(page);

        for b in &self.media {
            let placement = Placement::new();
            // Only a clip held locally is looped. A stream is played from its
            // address and left to end; see `cache_media`.
            let again = b.item.cache.is_some().then(|| b.src.clone());
            // The decoder's pads only exist once it has connected, so they are
            // watched as they appear. The same caps test `route_pads` makes.
            let (me, id) = (placement.clone(), id.clone());
            b.src.connect_pad_added(move |_el, pad| {
                // By name as well as by caps: a decoder can announce a pad
                // before its caps are known, and one classified by caps alone
                // was skipped here, never placed, and every frame of it late.
                let name = pad.name();
                let media = pad
                    .current_caps()
                    .and_then(|c| c.structure(0).map(|s| s.name().to_string()))
                    .unwrap_or_default();
                if name.starts_with("video") || media.starts_with("video/") {
                    me.watch(&id, Stream::Video, pad, again.clone());
                } else if name.starts_with("audio") || media.starts_with("audio/") {
                    me.watch(&id, Stream::Audio, pad, again.clone());
                }
            });
            all.push(placement);
        }
        Ok(all)
    }
}

/// Where a layer's segment goes: the end of the round before when there is
/// one and it is still within reach, otherwise `now` with the stream's bias.
/// See `PAGE_LAG_NS`, `MEDIA_LEAD_NS` and `MEDIA_JOIN_SLACK_NS`.
fn biased(stream: Stream, now: gst::ClockTime, after: gst::ClockTime) -> gst::ClockTime {
    match stream {
        Stream::Page => now,
        Stream::Video | Stream::Audio => {
            if after.is_zero() {
                // A first round: nothing is flowing yet and the decoder is
                // still starting, so it is given the lead.
                now + gst::ClockTime::from_nseconds(MEDIA_LEAD_NS)
            } else if after + gst::ClockTime::from_nseconds(MEDIA_JOIN_SLACK_NS) >= now {
                after
            } else {
                // Later than the join can reach. Its data is already flowing,
                // held back only by the queue that still holds the round
                // before, so now is where this round really is; the lead on
                // top of that would be silence for nothing.
                now
            }
        }
    }
}

/// Move `pad`'s offset, remembering in `own_change` that the segment resend
/// this causes is ours. A change to the same value causes no resend and is
/// not marked, or the next real segment would be swallowed.
fn place_offset(pad: &gst::Pad, own_change: &AtomicBool, offset: gst::ClockTime) {
    let want = offset.nseconds() as i64;
    if pad.offset() != want {
        own_change.store(true, Ordering::SeqCst);
        pad.set_offset(want);
    }
}

/// Which of a layered source's streams a `Placement` probe is watching.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stream {
    /// The browser's rendering of the page. One segment, placed once.
    Page,
    /// The media's picture. Its buffers say where each round ends.
    Video,
    /// The media's sound. Placed where the picture is, not where it would put
    /// itself, so the two cannot drift apart round after round.
    Audio,
}

/// A media stream's place in `Placement::streams` and `Placement::ended`. The
/// page is none of them: it never ends while the source lives and it is not
/// part of the media's loop.
fn stream_bit(stream: Stream) -> u32 {
    match stream {
        Stream::Page => 0,
        Stream::Video => 1,
        Stream::Audio => 2,
    }
}

/// Where each layer sits on the composite's timeline.
///
/// Every layer stamps its frames from its own zero, and none of them starts
/// when the pipeline does: the browser takes seconds to launch and load, the
/// media takes a moment to connect, and the pipeline clock runs throughout. The
/// compositor is live and shows a frame when its running time comes up, so a
/// layer left on its own timeline is late by however long it took to appear,
/// and a live aggregator drops what is late. So as each segment passes on its
/// way to its compositor pad, that pad's offset moves the layer to that moment,
/// measured from when this pipeline started, which is where the compositor's
/// timeline begins. The offset is set upstream of the pad, while the
/// segment is still travelling, because one set after the segment has gone by
/// changes nothing. The same reason the mixer's `TimelineAligner` works the
/// way it does.
///
/// The media sends a new segment every time it goes round, which is every time
/// its decoder is started again from the local copy when the clip runs out (see
/// the end of stream handling in `watch`). That one is placed at the end
/// of the round before, measured from the picture's buffers, so the picture
/// carries straight on; or at now, if the next round was not ready in time, in
/// which case the gap is what it is and the log says so. The sound is placed
/// by the picture's end rather than its own, or a clip whose audio runs a few
/// milliseconds longer than its video would pull the two apart a little more
/// on every round.
struct Placement {
    /// Where the picture of the current round ends, in running time at its
    /// compositor pad, as far as has been seen.
    end: Mutex<gst::ClockTime>,
    /// Where the round before ended, frozen as the picture's next segment
    /// passes, for the sound to follow.
    prev_end: Mutex<gst::ClockTime>,
    video_rounds: AtomicU32,
    audio_rounds: AtomicU32,
    /// Which of the media's streams this placement watches, and which of them
    /// have reached the end of the round, as `stream_bit`. The decoder is
    /// started again when the picture ends, and the sound of the same round
    /// ends a moment later; pulling the decoder down the instant the picture
    /// is done cut whatever sound had not been pushed yet.
    streams: AtomicU32,
    ended: AtomicU32,
    /// A restart of the media decoder in flight. Picture and sound reach their
    /// end within a frame of each other and one restart is wanted, not two.
    restarting: AtomicBool,
    /// When this pipeline last started, which is where its compositor's own
    /// timeline begins. Not the pipeline's running time: an input pipeline
    /// runs on the programme's clock and base time, so that read as 151
    /// seconds on a rig that had been up that long, and the layers were then
    /// placed 151 seconds into the future. The source showed black for
    /// exactly that long, and the earlier runs had only looked right because
    /// the rig was a few seconds old.
    started: Mutex<Instant>,
}

impl Placement {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            end: Mutex::new(gst::ClockTime::ZERO),
            prev_end: Mutex::new(gst::ClockTime::ZERO),
            video_rounds: AtomicU32::new(0),
            audio_rounds: AtomicU32::new(0),
            streams: AtomicU32::new(0),
            ended: AtomicU32::new(0),
            restarting: AtomicBool::new(false),
            started: Mutex::new(Instant::now()),
        })
    }

    /// Forget everything. The pipeline has been restarted and its running time
    /// starts from zero again; an end remembered from before it would place
    /// the first frames minutes into the future.
    /// The composite's clock, read where the layers are placed on it.
    ///
    /// The pipeline's own running time, not time since this source started.
    /// The mixer gives every source pipeline the programme's clock and base
    /// time (see `mixer.rs`, where a new source is started), so inside a source
    /// running time is the programme's age, and a source added ten seconds
    /// into a broadcast has a compositor whose clock reads ten seconds when it
    /// starts. Placing against a stopwatch started with the source put every
    /// page frame that far into the past, and the compositor dropped them as
    /// old: the page stood still at its first frame. Video from a file got away
    /// with it by skipping ahead until it caught up, which hid the error.
    /// Before the pipeline is playing there is no running time; the stopwatch
    /// stands in, which only affects a segment that arrives during preroll.
    fn now(&self, pad: &gst::Pad) -> gst::ClockTime {
        pad.parent_element()
            .and_then(|el| el.current_running_time())
            .unwrap_or_else(|| gst::ClockTime::from_nseconds(self.started.lock().elapsed().as_nanos() as u64))
    }

    fn reset(&self) {
        *self.end.lock() = gst::ClockTime::ZERO;
        *self.prev_end.lock() = gst::ClockTime::ZERO;
        self.video_rounds.store(0, Ordering::SeqCst);
        self.audio_rounds.store(0, Ordering::SeqCst);
        self.ended.store(0, Ordering::SeqCst);
        self.restarting.store(false, Ordering::SeqCst);
        *self.started.lock() = Instant::now();
    }

    /// Watch the segments and buffers passing `probe_on` and keep `target`,
    /// downstream of it, placed.
    fn watch(self: &Arc<Self>, id: &SourceId, stream: Stream, probe_on: &gst::Pad, again: Option<gst::Element>) {
        // The offset goes on this very pad, a source pad. Set on a sink pad
        // downstream, an offset takes effect only if the segment has not got
        // there yet, and where nothing sits between the two it always had: the
        // page's frames and the media's sound were each placed and then
        // dropped as old, every one. A source pad resends its segment with the
        // new offset before its next buffer, whenever the offset changes.
        self.streams.fetch_or(stream_bit(stream), Ordering::SeqCst);
        let (me, id, target) = (self.clone(), id.clone(), probe_on.clone());
        // This pad's segment, kept to turn buffer times into running time,
        // with the offset the pad had already folded into it. A pad applies
        // its offset to a segment before any probe sees it (gstpad.c,
        // gst_pad_push_event_unchecked), so the segment stored here is the
        // shifted one, and a running time read from it includes the last
        // placement. Taking that placement back out gives the stream's own
        // time, which is what a new placement must be computed from: computed
        // from the shifted time instead, each placement undid the one before
        // it, and half the page's frames landed at the start of time.
        let segment: Mutex<Option<(gst::FormattedSegment<gst::ClockTime>, gst::ClockTime)>> = Mutex::new(None);
        // Placed at the segment, which is the moment the offset can still take
        // effect on the pad downstream, and refined on the first buffer if its
        // timestamp within the segment is not zero, which a demuxer's segment
        // normally makes it. That refinement only works where a queue sits
        // between here and the pad, so the layers are probed upstream of one.
        let pending = std::sync::atomic::AtomicBool::new(false);
        let placed_at = Mutex::new(gst::ClockTime::ZERO);
        let placed_after = Mutex::new(gst::ClockTime::ZERO);
        // Changing this pad's offset makes it resend its segment, and that
        // resend comes straight back through this probe. Left alone it read as
        // a new segment, was placed again, changed the offset again, and so on
        // for every buffer: a dozen corrections a second and no layer ever
        // settled. Each change of ours is marked and its one resend ignored.
        let own_change = AtomicBool::new(false);
        // Frames seen, for the page's occasional account of itself in the log.
        let frames = std::sync::atomic::AtomicU64::new(0);
        probe_on.add_probe(
            gst::PadProbeType::EVENT_DOWNSTREAM | gst::PadProbeType::BUFFER,
            move |_p, info| {
                match &info.data {
                    Some(gst::PadProbeData::Event(e)) => {
                        if let gst::EventView::Segment(sg) = e.view() {
                            let folded = gst::ClockTime::from_nseconds(target.offset().max(0) as u64);
                            *segment.lock() = sg
                                .segment()
                                .downcast_ref::<gst::ClockTime>()
                                .cloned()
                                .map(|sg| (sg, folded));
                            if own_change.swap(false, Ordering::SeqCst) {
                                // The resend our own offset change caused.
                                return gst::PadProbeReturn::Ok;
                            }
                            let now = me.now(&target);
                            let after = match stream {
                                Stream::Page => gst::ClockTime::ZERO,
                                Stream::Video => {
                                    let end = *me.end.lock();
                                    *me.prev_end.lock() = end;
                                    me.video_rounds.fetch_add(1, Ordering::SeqCst);
                                    end
                                }
                                Stream::Audio => {
                                    // The picture's end. If the picture's own
                                    // new round has already begun it is frozen
                                    // in `prev_end`; if not, the old picture has
                                    // fully drained by the time the new sound
                                    // arrives, so `end` is final.
                                    let k = me.audio_rounds.fetch_add(1, Ordering::SeqCst) + 1;
                                    if me.video_rounds.load(Ordering::SeqCst) >= k {
                                        *me.prev_end.lock()
                                    } else {
                                        *me.end.lock()
                                    }
                                }
                            };
                            let place = biased(stream, now, after);
                            place_offset(&target, &own_change, place);
                            *placed_at.lock() = place;
                            *placed_after.lock() = after;
                            pending.store(true, Ordering::SeqCst);
                            if after == gst::ClockTime::ZERO {
                                info!(
                                    source = %id,
                                    layer = ?stream,
                                    at_ms = place.mseconds(),
                                    "layer placed on the composite's timeline"
                                );
                            } else {
                                info!(
                                    source = %id,
                                    layer = ?stream,
                                    at_ms = place.mseconds(),
                                    // What the viewer sees: how far past the
                                    // end of the round before this one starts,
                                    // not how late its first buffer was.
                                    gap_ms = place.saturating_sub(after).mseconds(),
                                    late_ms = now.saturating_sub(after).mseconds(),
                                    "placed the next round of the page's media"
                                );
                            }
                        } else if let gst::EventView::Eos(_) = e.view() {
                            // The clip has run out. For one held locally the
                            // decoder is started again from the copy, and the
                            // end of stream is kept from the compositor, which
                            // would otherwise mark the layer finished and
                            // ignore everything after. A stream is left to end.
                            let Some(el) = again.as_ref() else {
                                if stream == Stream::Page {
                                    // The browser drawing the page has gone: it
                                    // crashed, or its container was stopped. Left
                                    // alone, the compositor marks this one pad
                                    // finished and goes on compositing the videos
                                    // over black, and the source reports itself
                                    // live for as long as they play. That is the
                                    // one failure a viewer sees and nothing
                                    // reports. Posted as an error on the bus, it
                                    // is the supervisor's ordinary restart: a
                                    // fresh browser, the same source.
                                    if let Some(parent) = _p.parent_element() {
                                        warn!(source = %id, "the page's browser stopped; restarting the source");
                                        let msg = gst::message::Error::builder(
                                            gst::StreamError::Failed,
                                            "the page's browser stopped",
                                        )
                                        .src(&parent)
                                        .build();
                                        let _ = parent.post_message(msg);
                                    }
                                }
                                return gst::PadProbeReturn::Ok;
                            };
                            me.ended.fetch_or(stream_bit(stream), Ordering::SeqCst);
                            if stream == Stream::Video
                                && !me.restarting.swap(true, Ordering::SeqCst)
                            {
                                let (el, me, id) = (el.clone(), me.clone(), id.clone());
                                // Off the streaming thread. A state change made
                                // from a probe on the element's own pad deadlocks
                                // against the thread it is asking to stop.
                                std::thread::spawn(move || {
                                    let started = Instant::now();
                                    // The picture and the sound each have a
                                    // queue of the same length, so they reach
                                    // the end of the round within a moment of
                                    // each other; wait for the sound before
                                    // pulling the decoder down, or the last of
                                    // it never gets pushed. Bounded, so a
                                    // stream that never ends cannot stop the
                                    // loop for good.
                                    let want = me.streams.load(Ordering::SeqCst);
                                    while me.ended.load(Ordering::SeqCst) != want
                                        && started.elapsed() < MEDIA_END_WAIT
                                    {
                                        std::thread::sleep(Duration::from_millis(10));
                                    }
                                    me.ended.store(0, Ordering::SeqCst);
                                    let waited = started.elapsed();
                                    let down = el.set_state(gst::State::Null);
                                    if down.is_ok() && el.sync_state_with_parent().is_ok() {
                                        info!(
                                            source = %id,
                                            took_ms = started.elapsed().as_millis() as u64,
                                            waited_for_sound_ms = waited.as_millis() as u64,
                                            "started the page's media again from its local copy"
                                        );
                                    } else {
                                        warn!(
                                            source = %id,
                                            ?down,
                                            "the page's media could not be started again; the page stays over its last frame"
                                        );
                                    }
                                    me.restarting.store(false, Ordering::SeqCst);
                                });
                            }
                            return gst::PadProbeReturn::Drop;
                        }
                    }
                    Some(gst::PadProbeData::Buffer(b)) => {
                        let Some(pts) = b.pts() else {
                            return gst::PadProbeReturn::Ok;
                        };
                        let guard = segment.lock();
                        let Some((seg, folded)) = guard.as_ref() else {
                            return gst::PadProbeReturn::Ok;
                        };
                        // The stream's own running time, placement taken out.
                        let rt = seg
                            .to_running_time(pts)
                            .unwrap_or(gst::ClockTime::ZERO)
                            .saturating_sub(*folded);
                        let now = me.now(&target);
                        if stream == Stream::Page {
                            // The page is chrome, and the right time to show a
                            // frame of it is the moment it arrives. So every
                            // frame is stamped to now as it passes. The branch
                            // behind it can run late by whatever the pipe and
                            // the machine make it, and nothing is ever old: the
                            // earlier way, placing the page once, had the
                            // compositor skipping 132 of 134 frames as late.
                            pending.store(false, Ordering::SeqCst);
                            let want = now.saturating_sub(rt);
                            let have = gst::ClockTime::from_nseconds(target.offset().max(0) as u64);
                            let drift = want.max(have) - want.min(have);
                            // Only when the frame would otherwise land outside a
                            // small window around now. Every change of offset
                            // resends the segment, and a page that stamped each
                            // frame sent the compositor two events per frame.
                            if drift > gst::ClockTime::from_nseconds(PAGE_DRIFT_NS) {
                                place_offset(&target, &own_change, want);
                            }
                            let n = frames.fetch_add(1, Ordering::SeqCst);
                            if n == 0 || n % 50 == 0 {
                                info!(
                                    source = %id,
                                    frame = n,
                                    own_ms = rt.mseconds(),
                                    at_ms = (rt + gst::ClockTime::from_nseconds(target.offset().max(0) as u64)).mseconds(),
                                    now_ms = now.mseconds(),
                                    restamped = drift > gst::ClockTime::from_nseconds(PAGE_DRIFT_NS),
                                    "page frame through the placement probe"
                                );
                            }
                        } else if pending.swap(false, Ordering::SeqCst) {
                            // The segment said where the layer starts; the first
                            // buffer says when it really arrived and what its own
                            // clock read. Sound turns up a third of a second after
                            // its segment while its decoder starts, and a sample
                            // that reaches the mix behind its output position is
                            // dropped. Place the buffer, not the segment: this pad
                            // resends the segment with the corrected offset before
                            // the buffer after this one.
                            let placed = *placed_at.lock();
                            let after = *placed_after.lock();
                            // A round that has one before it keeps the join the
                            // segment chose. The sound's first buffer can be
                            // half a second behind its own segment while the
                            // decoder starts, and biasing it to now all over
                            // again opened exactly the gap the join is there to
                            // close: the picture carried on and the sound came
                            // back three quarters of a second later. Only a
                            // first round, which has nothing to join, is placed
                            // by its first buffer.
                            let place = if after.is_zero() { biased(stream, now, after) } else { placed };
                            place_offset(&target, &own_change, place.saturating_sub(rt));
                            if now > placed + gst::ClockTime::from_mseconds(100) || rt > gst::ClockTime::from_mseconds(20) {
                                info!(
                                    source = %id,
                                    layer = ?stream,
                                    at_ms = place.mseconds(),
                                    first_ms = rt.mseconds(),
                                    arrived_late_ms = now.saturating_sub(placed).mseconds(),
                                    "placement corrected by the first buffer"
                                );
                            }
                        }
                        if stream == Stream::Video {
                            // Running time at `target`, where the offset is
                            // applied, not here.
                            let dur = b.duration().unwrap_or(gst::ClockTime::ZERO);
                            let at = rt + dur
                                + gst::ClockTime::from_nseconds(target.offset().max(0) as u64);
                            let mut end = me.end.lock();
                            if at > *end {
                                *end = at;
                            }
                        }
                    }
                    _ => {}
                }
                gst::PadProbeReturn::Ok
            },
        );
    }
}



impl InputPipeline {
    /// The sidecar command that would ask this source's page what it plays.
    ///
    /// Some only for a website with `superimpose = "auto"` and a sidecar to
    /// run. The mixer runs the probe itself, on a thread of its own, and hands
    /// the answer to `build_kind` as `overlay`: launching a browser and waiting
    /// for a page to start playing takes seconds, and the mixer thread is what
    /// answers every other command in the meantime. A configured sidecar that
    /// is missing gives None here too, so that `build_kind` gets to fail with
    /// its usual message rather than the failure being swallowed.
    pub fn media_probe_spec(
        cfg: &SourceConfig,
        canvas: &CanvasCaps,
        browser: &BrowserConfig,
    ) -> Option<ExecSpec> {
        if SourceKind::detect(&cfg.uri) != SourceKind::Web || cfg.superimpose != Superimpose::Auto {
            return None;
        }
        ExecSpec::browser(&cfg.uri, canvas, browser).ok().flatten()
    }

    /// Build the pipeline in NULL state. Call `start` to run it.
    #[cfg(test)]
    pub fn build(
        cfg: &SourceConfig,
        canvas: &CanvasCaps,
        backends: &Backends,
        thumb_fps: i32,
        origin: Instant,
    ) -> Result<Self> {
        Self::build_kind(
            cfg,
            canvas,
            backends,
            thumb_fps,
            origin,
            SourceKind::Rtmp,
            false,
            &BrowserConfig::default(),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_kind(
        cfg: &SourceConfig,
        canvas: &CanvasCaps,
        backends: &Backends,
        thumb_fps: i32,
        origin: Instant,
        kind: SourceKind,
        allow_exec: bool,
        browser: &BrowserConfig,
        overlay: Option<MediaReport>,
    ) -> Result<Self> {
        let id = cfg.id.clone();
        let mut exec_child: Option<ExecChild> = None;
        // A page is rendered by the sidecar when there is one, and the sidecar
        // is just another process writing a container to stdout. From here on
        // such a source is an exec source in every respect.
        let mut exec = match kind {
            SourceKind::Exec => Some(ExecSpec::from_uri(&cfg.uri, allow_exec)?),
            SourceKind::Web => ExecSpec::browser(&cfg.uri, canvas, browser)?,
            _ => None,
        };

        // A page asking for `superimpose = "auto"` has already been probed by
        // the mixer, off the mixer's own thread, and `overlay` is what the page
        // said it plays. With a report the source is built as two layers and
        // the browser stops decoding video altogether. Without one, which is
        // every page feeding its own player from JavaScript, nothing below this
        // changes and the page is rendered whole exactly as before. A report
        // only counts for a page the sidecar renders: nothing else has a
        // browser to take the video back from.
        let overlay = match (kind, exec.as_ref()) {
            (SourceKind::Web, Some(_)) => overlay,
            _ => None,
        };
        if let (Some(spec), true) = (exec.as_mut(), overlay.is_some()) {
            // `--transparent` gives the page a real alpha channel, and with
            // `--detect-media` it also hides and pauses the video the mixer is
            // taking over, so Chromium neither paints nor decodes it. It costs
            // the page's audio, which is why the media's audio is used instead.
            spec.argv.push("--transparent".into());
            spec.argv.push("--detect-media".into());
            // And it draws slower, because now it is only drawing chrome. An
            // alpha frame is 4 bytes a pixel where I420 is 1.5, so a 720p page
            // at the canvas rate would put 107 MB/s down a pipe that carries
            // 41 MB/s for an ordinary source. See `default_overlay_fps`.
            spec.set_fps(browser.overlay_fps);
        }
        let exec = exec;

        let kind = if exec.is_some() { SourceKind::Exec } else { kind };
        let pipeline = gst::Pipeline::with_name(&format!("input-{id}"));
        let health = SourceHealth::new(origin);
        let rtmp_head_early = kind == SourceKind::Rtmp;

        // --- ingest -------------------------------------------------------
        // An exec source hands us a container, so it needs demuxing and
        // decoding; `decodebin` does both and respects the decoder ranks set by
        // the hardware probe.
        let decode_bin = (kind == SourceKind::Exec)
            .then(|| make("decodebin", &format!("{id}-decode")))
            .transpose()?;

        // The second decoder and the compositor that joins the two layers.
        // Built only when the probe above found something to decode.
        let layers = overlay
            .as_ref()
            .map(|r| Layers::build(&id, r))
            .transpose()?;

        let src = match kind {
            SourceKind::Rtmp => make_rtmp_source(cfg.rtmp_client.first_element(), &id, &cfg.uri)?,
            SourceKind::Live | SourceKind::File => {
                let el = make("uridecodebin", &format!("{id}-src-uri"))?;
                el.set_property("uri", to_uri(&cfg.uri));
                crate::probe::set_bool(&el, "use-buffering", true);
                el
            }
            SourceKind::Web => make_web_source(&id, &cfg.uri)?,
            SourceKind::Exec => {
                let spec = exec.as_ref().expect("exec kind implies a command");
                let (el, child) = make_exec_source(&id, spec)?;
                exec_child = Some(child);
                el
            }
        };
        // A short ingest queue decouples the network thread from decoding.
        let src_queue = rtmp_head_early
            .then(|| gstutil::queue_time(&format!("{id}-ingest-q"), 2.0, false))
            .transpose()?;
        let demux = rtmp_head_early
            .then(|| make("flvdemux", &format!("{id}-demux")))
            .transpose()?;

        // --- video normalisation -----------------------------------------
        // A live RTMP source is parsed and decoded explicitly so we control
        // which decoder is used. `uridecodebin` has already produced raw video
        // by the time it reaches us, so it skips straight to the normaliser.
        let rtmp_head = rtmp_head_early;
        let h264parse = rtmp_head
            .then(|| make("h264parse", &format!("{id}-h264parse")))
            .transpose()?;
        let decoder = rtmp_head
            .then(|| make(backends.video_decode.element, &format!("{id}-vdec")))
            .transpose()?;
        let download = match (rtmp_head, backends.video_decode.download) {
            (true, Some(f)) if crate::probe::exists(f) => Some(make(f, &format!("{id}-vdl"))?),
            (true, Some(f)) => {
                debug!(source = %id, element = f, "download element absent, relying on caps negotiation");
                None
            }
            _ => None,
        };
        let vrate = make("videorate", &format!("{id}-vrate"))?;
        // Fill gaps rather than letting the framerate sag. A source that sends
        // 28 fps into a 30 fps canvas must still produce 30 fps.
        crate::probe::set_bool(&vrate, "skip-to-first", true);
        crate::probe::set_bool(&vrate, "drop-only", false);
        let vconv = make("videoconvert", &format!("{id}-vconv"))?;
        // The converter is where a decoder's half-filled colour tag would be
        // acted on, so the tag is completed just before it.
        gstutil::assume_broadcast_colorimetry(&vconv, "sink")?;
        let vscale = make("videoscale", &format!("{id}-vscale"))?;
        let vcaps = gstutil::capsfilter(&format!("{id}-vcaps"), &canvas.video())?;
        // livesync exists to absorb drift and gaps in a *live* stream. A file
        // has neither, and its timestamps start at zero while the programme's
        // running time is minutes in, so livesync judges every early frame late
        // and discards it: an eight second ad lost its first 1.4 seconds. Media
        // sources are rebased on the mixer pad instead.
        let vsync = if kind.wants_livesync() {
            optional_livesync(&format!("{id}-vsync"))?
        } else {
            None
        };
        // Asks the browser to draw at the canvas size rather than rendering
        // small and being scaled up. Sits after the GL download, so the size it
        // asks for negotiates back upstream to the renderer.
        let web_caps = (kind == SourceKind::Web)
            .then(|| gstutil::capsfilter(&format!("{id}-web-caps"), &web_render_caps(canvas)))
            .transpose()?;

        // `wpesrc` hands out RGBA in GL memory, so it has to come back to
        // system memory before the normaliser can touch it. Verified against
        // the real element: its video caps are video/x-raw(memory:GLMemory).
        let (gl_convert, gl_download) = if kind == SourceKind::Web {
            anyhow::ensure!(
                crate::probe::exists("glcolorconvert") && crate::probe::exists("gldownload"),
                "rendering web pages needs the GStreamer OpenGL elements \
                 (gstreamer1.0-gl on Debian and Ubuntu). wpesrc draws into GL memory, \
                 so there is no software-only path."
            );
            (
                Some(make("glcolorconvert", &format!("{id}-gl-conv"))?),
                Some(make("gldownload", &format!("{id}-gl-dl"))?),
            )
        } else {
            (None, None)
        };

        let vtee = make("tee", &format!("{id}-vtee"))?;
        vtee.set_property("allow-not-linked", true);

        let vprog_q = gstutil::queue_thread(&format!("{id}-vprog-q"))?;
        let video_proxy = make("proxysink", &format!("{id}-vproxy"))?;

        let vthumb_q = gstutil::queue_thread(&format!("{id}-vthumb-q"))?;
        let tscale = make("videoscale", &format!("{id}-tscale"))?;
        let trate = make("videorate", &format!("{id}-trate"))?;
        let tcaps = gstutil::capsfilter(
            &format!("{id}-tcaps"),
            &CanvasCaps::video_at(THUMB_WIDTH, THUMB_HEIGHT, gst::Fraction::new(thumb_fps, 1)),
        )?;
        let thumb_proxy = make("proxysink", &format!("{id}-tproxy"))?;

        // --- audio normalisation -----------------------------------------
        let aacparse = rtmp_head
            .then(|| make("aacparse", &format!("{id}-aacparse")))
            .transpose()?;
        let adec = rtmp_head
            .then(|| make(backends.audio_decode, &format!("{id}-adec")))
            .transpose()?;
        let aconv = make("audioconvert", &format!("{id}-aconv"))?;
        let ares = make("audioresample", &format!("{id}-ares"))?;
        let acaps = gstutil::capsfilter(&format!("{id}-acaps"), &canvas.audio())?;
        let async_: Option<gst::Element> = None; // EXPERIMENT: no audio livesync
        let audio_proxy = make("proxysink", &format!("{id}-aproxy"))?;

        // --- assemble -----------------------------------------------------
        let mut all: Vec<&gst::Element> = vec![
            &src, &vrate, &vconv, &vscale, &vcaps,
            &vtee, &vprog_q, &video_proxy, &vthumb_q, &tscale, &trate, &tcaps, &thumb_proxy,
            &aconv, &ares, &acaps, &audio_proxy,
        ];
        for el in [
            &src_queue, &demux, &h264parse, &decoder, &aacparse, &adec, &web_caps, &decode_bin,
            &gl_convert, &gl_download,
        ]
        .into_iter()
        .flatten()
        {
            all.push(el);
        }
        if let Some(d) = &download {
            all.push(d);
        }
        if let Some(s) = &vsync {
            all.push(s);
        }
        if let Some(s) = &async_ {
            all.push(s);
        }
        if let Some(l) = &layers {
            all.extend(l.elements());
        }
        pipeline.add_many(&all).context("adding input elements")?;

        if let (Some(q), Some(d)) = (&src_queue, &demux) {
            gst::Element::link_many([&src, q, d]).context("linking ingest")?;
        }
        if let Some(dec) = &decode_bin {
            gst::Element::link(&src, dec).context("linking exec source to decoder")?;
        }

        // Video chain: for RTMP, parse and decode first, optionally downloading
        // from GPU memory. Rate before scale so we convert as few frames as we
        // can. For a media URI the decoder has already run, so the chain starts
        // at the normaliser and both kinds converge on the same capsfilter.
        let mut vchain: Vec<&gst::Element> = Vec::new();
        if let (Some(p), Some(d)) = (&h264parse, &decoder) {
            vchain.push(p);
            vchain.push(d);
        }
        if let Some(d) = &download {
            vchain.push(d);
        }
        vchain.extend([&vrate, &vconv, &vscale, &vcaps]);
        if let Some(s) = &vsync {
            vchain.push(s);
        }
        vchain.push(&vtee);
        gst::Element::link_many(&vchain).context("linking video normaliser")?;

        gst::Element::link_many([&vtee, &vprog_q, &video_proxy]).context("linking program video branch")?;
        gst::Element::link_many([&vtee, &vthumb_q, &trate, &tscale, &tcaps, &thumb_proxy])
            .context("linking thumbnail branch")?;

        let mut achain: Vec<&gst::Element> = Vec::new();
        if let (Some(p), Some(d)) = (&aacparse, &adec) {
            achain.push(p);
            achain.push(d);
        }
        achain.extend([&aconv, &ares, &acaps]);
        if let Some(s) = &async_ {
            achain.push(s);
        }
        achain.push(&audio_proxy);
        gst::Element::link_many(&achain).context("linking audio normaliser")?;

        // --- liveness probes ---------------------------------------------
        // Placed after normalisation so they count frames the mixer can
        // actually use, not frames that arrived and failed to convert.
        // Probe the proxy sinks, the last thing before the mixer, rather than
        // the middle of the chain. A probe further upstream reports a source as
        // healthy while an element downstream of it silently discards
        // everything, which is exactly how the cameras appeared to have audio
        // while the programme carried silence.
        install_buffer_probe(&video_proxy, "sink", {
            let h = health.clone();
            move || h.mark_video()
        })?;
        install_buffer_probe(&audio_proxy, "sink", {
            let h = health.clone();
            move || h.mark_audio()
        })?;

        // --- dynamic demuxer pads ----------------------------------------
        let has_video = Arc::new(AtomicBool::new(false));
        let has_audio = Arc::new(AtomicBool::new(false));

        // The browser's video comes out of GL memory, is downloaded, then meets
        // the render capsfilter on its way into the normaliser.
        if let (Some(conv), Some(dl), Some(caps)) = (&gl_convert, &gl_download, &web_caps) {
            gst::Element::link_many([conv, dl, caps, &vrate])
                .context("linking the web render chain")?;
        }

        // Entry points for dynamically added pads: the overlay queue when the
        // page is being drawn over its own video, the parsers for RTMP, the GL
        // conversion for a web page, the normaliser itself for an
        // already-decoded media URI.
        let video_entry = layers
            .as_ref()
            .map(|l| l.over_q.clone())
            .or_else(|| h264parse.clone())
            .or_else(|| gl_convert.clone())
            .unwrap_or_else(|| vrate.clone());
        let audio_entry = aacparse.clone().unwrap_or_else(|| aconv.clone());

        // `wpesrc`'s video pad is a static pad named `video`, not `src`, and it
        // is always present. Its audio arrives later on `audio_%u`, which the
        // pad-added handler below picks up by caps. Verified by inspecting the
        // real element on Linux; the earlier guess at `src` linked nothing at
        // all and would have produced a black source.
        if kind == SourceKind::Web {
            let out = src
                .static_pad("video")
                .context("wpesrc has no `video` pad; the plugin version may differ")?;
            let entry = video_entry
                .static_pad("sink")
                .context("web render chain has no sink pad")?;
            out.link(&entry).context("linking web source video")?;
            has_video.store(true, Ordering::Relaxed);
        }

        let dynamic = demux
            .clone()
            .or_else(|| decode_bin.clone())
            .unwrap_or_else(|| src.clone());
        route_pads(
            &dynamic,
            &id,
            Some(video_entry),
            // On the layered path this decoder is the page, and whatever the
            // page still plays once its videos are taken over goes into the
            // mix with them; see `Layers::link`.
            Some(layers.as_ref().map(|l| l.page_aconv.clone()).unwrap_or_else(|| audio_entry.clone())),
            &has_video,
            &has_audio,
        );
        let mut placement = Vec::new();
        if let Some(l) = &layers {
            l.link(canvas, &vrate, &audio_entry)
                .context("linking the superimposed layers")?;
            // Before the routing below, so the placement probes are on each new
            // pad before anything is linked to it.
            placement = l.place_in_time(&id, &dynamic)?;
            // The page's videos, decoded here, each into its own branch.
            for b in &l.media {
                route_pads(
                    &b.src,
                    &id,
                    Some(b.conv.clone()),
                    Some(b.aq.clone()),
                    &has_video,
                    &has_audio,
                );
            }
        }

        Ok(Self {
            id,
            config: cfg.clone(),
            pipeline,
            health,
            video_proxy,
            thumb_proxy,
            audio_proxy,
            has_video,
            has_audio,
            superimposed: layers.is_some(),
            levels: layers.as_ref().map(|l| l.levels()),
            placement,
            media_cache: overlay
                .as_ref()
                .map(|r| r.media.iter().filter_map(|m| m.cache.clone()).collect())
                .unwrap_or_default(),
            failed: Arc::new(AtomicBool::new(false)),
            seekable: Mutex::new(None),
            source: Mutex::new(src),
            src_queue,
            fallback_used: AtomicBool::new(false),
            restart_armed: AtomicBool::new(false),
            exec_child: Mutex::new(exec_child),
            exec,
            kind,
        })
    }

    /// Bring the pipeline up to PAUSED and wait briefly for it to preroll.
    ///
    /// Used for a scheduled ad so its first frame is decoded and waiting before
    /// the cue arrives, rather than the break opening on a few frames of black.
    pub fn preroll(&self, timeout: std::time::Duration) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Paused)
            .with_context(|| format!("prerolling {}", self.id))?;
        let (res, state, _) = self.pipeline.state(gst::ClockTime::from_mseconds(
            timeout.as_millis().min(u128::from(u64::MAX)) as u64,
        ));
        match res {
            Err(e) => anyhow::bail!("{} failed to preroll: {e:?}", self.id),
            Ok(_) if state != gst::State::Paused => {
                anyhow::bail!("{} did not reach PAUSED (got {state:?})", self.id)
            }
            Ok(_) => Ok(()),
        }
    }

    pub fn start(&self) -> Result<()> {
        // The layers' clock starts here, not when the pipeline was built. Two
        // seconds passed between the two while the sidecar's container was
        // launched, and sound placed by the build clock landed two seconds
        // behind the mix's own timeline and was dropped, every buffer of it.
        // Video survived the same error only because a compositor shows a late
        // frame anyway.
        for p in &self.placement {
            p.reset();
        }
        self.pipeline
            .set_state(gst::State::Playing)
            .with_context(|| format!("starting input pipeline for {}", self.id))?;
        Ok(())
    }

    pub fn stop(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        self.kill_exec_child();
        for f in &self.media_cache {
            let _ = std::fs::remove_file(f);
        }
    }

    fn kill_exec_child(&self) {
        let Some(mut held) = self.exec_child.lock().take() else { return };
        let env = self.exec.as_ref().map(|s| s.env.clone()).unwrap_or_default();
        stop_child(held.child.id(), &env);
        // Where there is no process group to signal, end the process itself.
        // Chromium's own helper processes watch their parent and follow it.
        #[cfg(not(unix))]
        let _ = held.child.kill();
        let _ = held.child.wait();
        // Then everything of ours that the child's death does not close by
        // itself: the thread reading its stderr, which a surviving grandchild
        // holding the write end would otherwise keep alive for good, and the
        // descriptor the source element was reading, which `fdsrc` never
        // closes because it did not open it. Two descriptors a build, measured
        // over thirty add and remove cycles on this machine before and after.
        if let Some(r) = held.stderr.as_mut() {
            r.stop();
        }
        held.stdout.take();
        debug!(source = %self.id, "stopped exec child process");
    }

    pub fn mark_failed(&self) {
        self.failed.store(true, Ordering::Relaxed);
    }

    /// Swap in the other RTMP client implementation, once.
    ///
    /// Returns false when the configuration pins a client, or when the swap has
    /// already been used. Called only for a source that has produced no media
    /// at all, so nothing downstream has state to lose.
    pub fn try_fallback_client(&self) -> Result<bool> {
        let Some(queue) = self.src_queue.clone() else {
            return Ok(false); // Not an RTMP source; there is nothing to swap.
        };
        let Some(element) = self.config.rtmp_client.fallback_element() else {
            return Ok(false);
        };
        if self.fallback_used.swap(true, Ordering::SeqCst) {
            return Ok(false);
        }

        warn!(
            source = %self.id,
            from = self.config.rtmp_client.first_element(),
            to = element,
            "no media arrived from this RTMP client, trying the other implementation"
        );

        self.pipeline.set_state(gst::State::Null).ok();
        let fresh = make_rtmp_source(element, &self.id, &self.config.uri)?;
        {
            let mut current = self.source.lock();
            self.pipeline.remove(&*current).context("removing the old rtmp source")?;
            self.pipeline.add(&fresh).context("adding the replacement rtmp source")?;
            fresh.link(&queue).context("linking the replacement rtmp source")?;
            *current = fresh;
        }
        self.health.reset();
        self.start()?;
        Ok(true)
    }

    /// True if this source has never produced a single frame or sample.
    pub fn never_connected(&self) -> bool {
        !self.health.saw_video() && !self.health.saw_audio()
    }

    /// Claim the right to schedule a restart.
    ///
    /// Returns false if one is already pending, so the burst of errors a dead
    /// server produces collapses into a single retry.
    pub fn try_arm_restart(&self) -> bool {
        self.restart_armed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Tear the pipeline down and bring it back up. Used by the supervisor
    /// after an error or a prolonged stall. The program pipeline is untouched
    /// throughout: its proxysrc simply produces nothing for the duration, and
    /// the compositor's `force-live` keeps the canvas ticking.
    pub fn restart(&self) -> Result<()> {
        info!(source = %self.id, "restarting input pipeline");
        self.restart_armed.store(false, Ordering::SeqCst);
        self.pipeline.set_state(gst::State::Null).ok();
        for p in &self.placement {
            p.reset();
        }

        // An exec source's process has to come back with its pipeline. The old
        // one is killed and a fresh fd handed to the same fdsrc, so a command
        // that exits is simply run again.
        if let Some(spec) = &self.exec {
            self.kill_exec_child();
            match spawn_exec(&self.id, spec) {
                Ok((out, child, stderr)) => {
                    let stdout = attach_exec_stdout(&self.id, &self.source.lock(), out);
                    *self.exec_child.lock() = Some(ExecChild { child, stdout, stderr });
                }
                Err(e) => {
                    warn!(source = %self.id, ?e, "could not restart exec source");
                    return Err(e);
                }
            }
        }
        self.health.reset();
        self.has_video.store(false, Ordering::Relaxed);
        self.has_audio.store(false, Ordering::Relaxed);
        self.failed.store(false, Ordering::Relaxed);
        self.start()
    }

    pub fn has_video(&self) -> bool {
        self.has_video.load(Ordering::Relaxed)
    }

    pub fn has_audio(&self) -> bool {
        self.has_audio.load(Ordering::Relaxed)
    }

    /// True when this source is running with the page's media decoded here
    /// rather than in the browser. Decided once when the pipeline is built.
    pub fn superimposed(&self) -> bool {
        self.superimposed
    }

    /// The page and media levels, when this source has them.
    pub fn levels(&self) -> Option<&AudioLevels> {
        self.levels.as_ref()
    }

    /// Whether this source can be scrubbed, as the pipeline last answered.
    ///
    /// False while nothing has answered yet, which is the honest reading: a
    /// scrubber on a source that turns out to be a live feed is worse than a
    /// scrubber that appears a moment late.
    pub fn seekable(&self) -> bool {
        self.seekable.lock().unwrap_or(false)
    }

    /// Ask the pipeline whether it can be scrubbed, unless it has already said.
    ///
    /// Asked rather than worked out from the URI, because the URI does not know.
    /// A clip fetched over HTTP from a server that refuses range requests cannot
    /// be seeked back to the start even though its address ends in `.mp4`, and a
    /// Python test server is exactly such a one: `cache_media` exists because of
    /// it. The query is the only thing that tells the two apart.
    ///
    /// The answer is kept once given. It cannot change while the pipeline plays
    /// the same URI, and the supervisor calls this twice a second for every
    /// source. An unanswered query is not a no: until the chain from the source
    /// element to the proxies is built there is nothing upstream to ask, so that
    /// case leaves the answer open and this gets asked again on the next tick.
    pub fn refresh_seekable(&self) -> bool {
        let mut known = self.seekable.lock();
        if let Some(answer) = *known {
            return answer;
        }
        let mut query = gst::query::Seeking::new(gst::Format::Time);
        if !self.pipeline.query(&mut query) {
            return false;
        }
        let (answer, _, _) = query.result();
        *known = Some(answer);
        debug!(source = %self.id, seekable = answer, "the pipeline says whether it can be scrubbed");
        answer
    }

    /// How far through this source is, in milliseconds. `None` while nothing
    /// upstream can say, which covers a pipeline that has not started yet.
    pub fn position_ms(&self) -> Option<u64> {
        self.pipeline.query_position::<gst::ClockTime>().map(|t| t.mseconds())
    }

    /// How long this source runs, in milliseconds. `None` on a live feed, which
    /// has no end, and on a file whose demuxer has not worked it out yet.
    pub fn duration_ms(&self) -> Option<u64> {
        self.pipeline.query_duration::<gst::ClockTime>().map(|t| t.mseconds())
    }

    /// Move this source to `position_ms` and answer with where it landed.
    ///
    /// Flushing, so that what an operator asked for arrives now rather than
    /// after the queues have played out the second or so they already hold.
    /// Accurate rather than fast, because someone dragging a scrubber is looking
    /// for a particular moment, and landing several seconds earlier on the
    /// previous keyframe reads as the control being broken.
    ///
    /// A position past the end is clamped to the duration rather than refused,
    /// for the reason `checked_gain` gives: an operator mid-broadcast has better
    /// things to do than read a validation error, and dragging a scrubber to the
    /// right hand end plainly means the end.
    ///
    /// The caller must reset this source's `TimelineAligner` first. A flushing
    /// seek restarts the segment, which makes the offset computed from the
    /// previous one wrong, and nothing here can see the aligner.
    pub fn seek_ms(&self, position_ms: u64) -> Result<u64> {
        // With no duration to clamp against, the ceiling is the largest time
        // GStreamer can express: `ClockTime::from_mseconds` panics past it, and
        // a request carrying a silly number must not take the mixer down.
        let wanted = position_ms.min(self.duration_ms().unwrap_or(gst::ClockTime::MAX.mseconds()));
        self.pipeline
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_mseconds(wanted),
            )
            .with_context(|| format!("seeking {} to {wanted}ms", self.id))?;
        // Read back rather than reported: the seek snaps to a key unit, so where
        // it landed and what was asked for are rarely the same millisecond.
        Ok(self.position_ms().unwrap_or(wanted))
    }

    pub fn observed_state(&self) -> SourceState {
        if self.failed.load(Ordering::Relaxed) {
            SourceState::Failed
        } else if self.health.is_stalled(self.config.stall_timeout_secs) {
            SourceState::Stalled
        } else if self.health.saw_video() || self.health.saw_audio() {
            SourceState::Live
        } else {
            SourceState::Connecting
        }
    }
}

/// The levels a superimposed source exposes.
///
/// A page drawn over video has two sounds: the page's own, which is the
/// commentary and anything the quiz plays, and one per video the mixer decodes
/// underneath. They arrive on separate branches, so they can be balanced
/// against each other. A whole page source has neither, because Chromium has
/// already mixed everything into one stream by the time the mixer sees it.
#[derive(Clone)]
pub struct AudioLevels {
    page: gst::Element,
    media: Vec<gst::Element>,
}

impl AudioLevels {
    /// Gain for the page's own sound. 1.0 leaves it alone, 0.0 silences it.
    pub fn set_page(&self, gain: f64) {
        self.page.set_property("volume", gain.clamp(0.0, 10.0));
    }

    /// Gain for one of the videos the mixer is decoding underneath.
    pub fn set_media(&self, index: usize, gain: f64) -> bool {
        match self.media.get(index) {
            Some(v) => {
                v.set_property("volume", gain.clamp(0.0, 10.0));
                true
            }
            None => false,
        }
    }

    pub fn page_gain(&self) -> f64 {
        self.page.property::<f64>("volume")
    }

    pub fn media_gains(&self) -> Vec<f64> {
        self.media.iter().map(|v| v.property::<f64>("volume")).collect()
    }

    pub fn media_count(&self) -> usize {
        self.media.len()
    }

    /// Apply a partial balance request and say where every channel ended up.
    ///
    /// Only the channels named move. The UI sends one fader at a time, so a
    /// request carrying a page gain and no media list must leave the videos
    /// where they are rather than resetting them to unity. Media gains are
    /// positional; a null entry, or a list shorter than the number of videos,
    /// leaves those channels alone for the same reason. A list longer than the number of
    /// videos is not an error: the page may have dropped a video since the UI
    /// last drew itself, and the reported levels say how many there really
    /// are.
    pub fn apply(&self, page: Option<f64>, media: &[Option<f64>]) -> SourceAudio {
        if let Some(gain) = page {
            self.set_page(gain);
        }
        // Positional and sparse: a null holds that channel where it is. The UI
        // moves one fader at a time and has no business restating the others,
        // and a short list cannot name the second video without also naming
        // the first.
        for (index, gain) in media.iter().enumerate() {
            if let Some(gain) = gain {
                self.set_media(index, *gain);
            }
        }
        self.report()
    }

    /// Where the channels sit now, read back off the elements rather than
    /// remembered, so a clamped request reports the gain that took effect.
    pub fn report(&self) -> SourceAudio {
        SourceAudio { page: self.page_gain(), media: self.media_gains() }
    }
}

/// Send a decoder's pads to the branches that want them, as they appear.
///
/// `flvdemux` names its pads; `uridecodebin` does not, so the media type on the
/// pad's caps is the fallback. The destinations are arguments rather than fixed
/// because a superimposed source runs two decoders in one bin, the page and its
/// video, and each one has its own branch to reach. A `None` destination means
/// that decoder's stream of that kind is deliberately dropped.
fn route_pads(
    el: &gst::Element,
    id: &SourceId,
    video: Option<gst::Element>,
    audio: Option<gst::Element>,
    has_video: &Arc<AtomicBool>,
    has_audio: &Arc<AtomicBool>,
) {
    let id = id.clone();
    let has_video = has_video.clone();
    let has_audio = has_audio.clone();
    el.connect_pad_added(move |_el, pad| {
        let name = pad.name();
        let media = pad
            .current_caps()
            .and_then(|c| c.structure(0).map(|s| s.name().to_string()))
            .unwrap_or_default();

        let is_video = name.starts_with("video") || media.starts_with("video/");
        let is_audio = name.starts_with("audio") || media.starts_with("audio/");

        let (target, seen) = if is_video {
            (video.as_ref(), &has_video)
        } else if is_audio {
            (audio.as_ref(), &has_audio)
        } else {
            debug!(source = %id, pad = %name, %media, "ignoring unrecognised pad");
            return;
        };
        let Some(target) = target else {
            debug!(source = %id, pad = %name, %media, "no branch wants this stream");
            return;
        };

        let Some(sink) = target.static_pad("sink") else { return };
        if sink.is_linked() {
            debug!(source = %id, pad = %name, "entry already linked, ignoring extra stream");
            return;
        }
        match pad.link(&sink) {
            Ok(_) => {
                seen.store(true, Ordering::Relaxed);
                info!(source = %id, pad = %name, %media, "linked source pad");
            }
            Err(e) => warn!(source = %id, pad = %name, ?e, "failed to link source pad"),
        }
    });
}

/// The private profile directory a sidecar of this pid would have made.
///
/// The sidecar picks it from its own pid (`browser/src/main.rs`, `opts`) and
/// removes it when its message loop ends. `stop_process_group` never lets it
/// get that far: SIGTERM is not handled inside CEF's loop and SIGKILL cannot
/// be, so every run left its profile behind. On air on 2026-09-12 that was
/// 1084 directories and 18 GB of a container's disk. Removing it here is the
/// only place that knows both the pid and that the process is now dead.
///
/// `TMPDIR` from the spec's environment where there is one, because the child
/// resolved its own temp directory with the environment we gave it; otherwise
/// ours. An operator who passes `--cache-dir` in `browser.args` puts the
/// profile somewhere this cannot predict, and then this is a no-op and the
/// sidecar's own cleanup is all there is.
fn browser_profile_dir(
    env: &std::collections::BTreeMap<String, String>,
    pid: u32,
) -> std::path::PathBuf {
    let base = env
        .get("TMPDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join(format!("lbx-browser-{pid}"))
}

/// Stop a child, everything it started, and the profile directory it was too
/// dead to remove itself.
fn stop_child(pid: u32, env: &std::collections::BTreeMap<String, String>) {
    stop_process_group(pid);
    remove_browser_profile(env, pid);
}

/// Take the profile directory of a sidecar that is already dead.
fn remove_browser_profile(env: &std::collections::BTreeMap<String, String>, pid: u32) {
    let dir = browser_profile_dir(env, pid);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => debug!(path = %dir.display(), "removed the sidecar's profile directory"),
        // Not a browser source, or the sidecar got there first. Either is fine;
        // anything else is worth knowing about, because it is disk that will
        // not come back on its own.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warn!(path = %dir.display(), ?e, "could not remove the sidecar's profile directory"),
    }
}

/// Stop a child and everything it started.
///
/// `Child::kill` sends SIGKILL to one process, which a shell script cannot trap
/// and which leaves its own children running. A capture script that starts a
/// browser and an X server would leak both on every restart. Signalling the
/// process group gives the script a chance to tidy up, then takes the whole
/// tree down whether it did or not.
#[cfg(unix)]
fn stop_process_group(pid: u32) {
    let pgid = pid as i32;
    unsafe {
        // Politely first, so traps run and children are cleaned up.
        libc::killpg(pgid, libc::SIGTERM);
    }
    // Give the group a moment to go quietly before insisting.
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if unsafe { libc::killpg(pgid, 0) } != 0 {
            return; // Group is gone.
        }
    }
    unsafe {
        libc::killpg(pgid, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn stop_process_group(_pid: u32) {}

/// A thread reading a child's stderr, and the means to end it.
///
/// A pipe read cannot be given a deadline, and the write end of a child's
/// stderr is not only the child's: Chromium's helper processes inherit it, and
/// on air on 2026-09-12 enough of them outlived the kill that the reader
/// threads never saw an end of file. Each survivor cost a thread and the read
/// end of a pipe, which is half of the two descriptors a rebuild leaked. So the
/// read is non-blocking with a poll in front of it, the thread checks a flag,
/// and `stop` joins it: when that returns, the descriptor is closed, whether or
/// not anything is still holding the other end.
struct StderrReader {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl StderrReader {
    /// Read `err` line by line, handing each to `on_line`, until the pipe ends
    /// or `stop` is called.
    fn spawn(
        name: String,
        err: std::process::ChildStderr,
        mut on_line: impl FnMut(&str) + Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let join = std::thread::Builder::new()
            .name(name)
            .spawn(move || drain_stderr(err, &flag, &mut on_line))
            .ok();
        Self { stop, join }
    }

    /// End the thread and close the pipe. Blocks for up to one poll interval.
    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for StderrReader {
    fn drop(&mut self) {
        self.stop();
    }
}

/// How long the reader parks between looks at the stop flag. Long enough that
/// an idle child costs nothing measurable, short enough that stopping a source
/// is not something an operator notices.
const STDERR_POLL_MS: i32 = 200;

#[cfg(unix)]
fn drain_stderr(
    mut err: std::process::ChildStderr,
    stop: &AtomicBool,
    on_line: &mut impl FnMut(&str),
) {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    let fd = err.as_raw_fd();
    // Non-blocking, so a read never parks this thread past the flag above.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
    let mut pending: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    while !stop.load(Ordering::SeqCst) {
        let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        let ready = unsafe { libc::poll(&mut pfd, 1, STDERR_POLL_MS) };
        if ready < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if ready == 0 {
            continue;
        }
        match err.read(&mut buf) {
            Ok(0) => break, // Every writer has let go.
            Ok(n) => {
                pending.extend_from_slice(&buf[..n]);
                while let Some(nl) = pending.iter().position(|b| *b == b'\n') {
                    let line: Vec<u8> = pending.drain(..=nl).collect();
                    on_line(String::from_utf8_lossy(&line[..nl]).trim_end().as_ref());
                }
                // A child writing megabytes without a newline must not grow
                // this without bound. Nothing either caller reads is near it.
                if pending.len() > 1 << 20 {
                    pending.clear();
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => break,
        }
    }
    if !pending.is_empty() {
        on_line(String::from_utf8_lossy(&pending).trim_end().as_ref());
    }
    // `err` drops here, and with it the read end of the pipe.
}

#[cfg(not(unix))]
fn drain_stderr(
    err: std::process::ChildStderr,
    _stop: &AtomicBool,
    on_line: &mut impl FnMut(&str),
) {
    use std::io::BufRead;
    for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
        on_line(&line);
    }
}

/// Reap orphans when this process is the container's init.
///
/// The mixer runs as PID 1 in its container, and PID 1 inherits every orphan
/// on the box. The sidecar's own grandchildren are orphaned the moment
/// `stop_process_group` takes their parent, and nothing was reaping them: on
/// air on 2026-09-12 there were 19,138 zombies after two hours, each one a
/// process table entry that the kernel will not release until somebody waits
/// on it. `init: true` in the compose file fixes it from outside; this fixes
/// it whether or not anybody remembered to.
///
/// A tick rather than a SIGCHLD handler on purpose. Almost nothing is legal
/// inside a signal handler, and the cost of asking once a second is one
/// syscall that returns immediately when there is nothing to collect.
#[cfg(unix)]
pub fn reap_orphans_if_init() {
    if unsafe { libc::getpid() } != 1 {
        return;
    }
    info!("running as pid 1: reaping orphaned processes");
    std::thread::Builder::new()
        .name("reaper".into())
        .spawn(|| loop {
            // `Child::wait` at the call sites may lose the race for a status
            // this collects first. Both of them ignore what it returns, which
            // is the only reason a blanket reaper is safe here.
            let mut reaped = 0u32;
            loop {
                let mut status = 0i32;
                let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
                if pid <= 0 {
                    break;
                }
                reaped += 1;
            }
            if reaped > 0 {
                debug!(reaped, "reaped orphaned processes");
            }
            std::thread::sleep(Duration::from_secs(1));
        })
        .ok();
}

#[cfg(not(unix))]
pub fn reap_orphans_if_init() {}

/// Start a command in its own process group, with its stderr on a pipe.
///
/// The process group is what makes stopping one work. A capture command is
/// often a script that starts a browser and an X server, and signalling only
/// the script leaves those orphaned; `stop_process_group` signals the tree.
/// Callers choose what happens to stdout, because a source reads it and the
/// media probe must throw it away.
fn exec_process(spec: &ExecSpec, stdout: std::process::Stdio) -> Result<std::process::Child> {
    let (program, args) = spec.argv.split_first().context("exec source has an empty command")?;

    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .envs(&spec.env)
        .stdout(stdout)
        // Keep the child's noise out of our own stderr; it is logged separately.
        .stderr(std::process::Stdio::piped());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn().with_context(|| format!("starting `{program}`"))
}

/// An exec child's stdout, on its way into GStreamer.
///
/// On unix the pipe's descriptor is handed to `fdsrc`, which reads it in the
/// streaming thread with no copy in between. Windows has no descriptor a
/// GStreamer element could read (the pipe is a HANDLE, and the C runtime's
/// descriptor table is per DLL), so there a thread of ours reads the pipe and
/// pushes into an `appsrc`. Same bytes, same downstream.
/// What the mixer holds on to for a running exec child. All three go together:
/// the process, the descriptor its picture comes down, and the thread reading
/// its complaints. Before this they were three separate lifetimes and two of
/// them outlived the process.
struct ExecChild {
    child: std::process::Child,
    /// The read end of the child's stdout, held for as long as the element
    /// reads it. See `ExecStdout`.
    stdout: ExecStdoutHeld,
    stderr: Option<StderrReader>,
}

/// Whatever has to be kept alive to keep the source element reading. On unix
/// that is the descriptor itself; on Windows a reader thread owns the pipe and
/// there is nothing left over to hold.
#[cfg(unix)]
type ExecStdoutHeld = Option<std::os::fd::OwnedFd>;
#[cfg(not(unix))]
type ExecStdoutHeld = Option<std::convert::Infallible>;

/// The descriptor stays owned. `into_raw_fd` gave it away, and `fdsrc` never
/// closes a descriptor it did not open itself (it only closes its own, from
/// the `fd://` URI handler), so every exec child ever started left the read
/// end of its stdout pipe open in this process: one of the two descriptors a
/// rebuild leaked on air on 2026-09-12. Held here instead, so that killing the
/// child closes it.
enum ExecStdout {
    #[cfg(unix)]
    Fd(std::os::fd::OwnedFd),
    #[cfg(not(unix))]
    Pipe(std::process::ChildStdout),
}

/// Start the command behind an `exec:` source and take its stdout.
///
/// Anything that can write a container to stdout becomes a source: ffmpeg, a
/// script, a purpose-built capture binary. Decoding happens downstream through
/// `decodebin`, which picks up the raised ranks of whatever hardware decoder
/// this machine has, so an exec source is accelerated on a GPU box and falls
/// back to software on one without, exactly like every other source.
fn spawn_exec(id: &str, spec: &ExecSpec) -> Result<(ExecStdout, std::process::Child, Option<StderrReader>)> {
    let mut child = exec_process(spec, std::process::Stdio::piped())?;

    let stderr = child.stderr.take().map(|err| {
        let name = id.to_string();
        StderrReader::spawn(format!("exec-stderr-{id}"), err, move |line| {
            debug!(source = %name, "{line}");
        })
    });

    let stdout = child.stdout.take().context("child produced no stdout")?;
    #[cfg(unix)]
    let out = ExecStdout::Fd(std::os::fd::OwnedFd::from(stdout));
    #[cfg(not(unix))]
    let out = ExecStdout::Pipe(stdout);
    let program = spec.argv.first().map(String::as_str).unwrap_or_default();
    info!(source = %id, %program, "started exec source");
    Ok((out, child, stderr))
}

/// Reads of the exec pipe, in frame-sized bites rather than the 4 KB default.
///
/// A source writing raw video moves a lot through that pipe: the browser
/// sidecar in transparent mode is 1280x720 AYUV at 30, which is 107 MB a
/// second. Measured against it on this machine, 4 KB reads carried 93 MB/s
/// and 4 MB reads carried 177 MB/s, and the difference is the difference
/// between the page keeping up and falling behind. A short read still
/// returns immediately, so a source producing very little is not made to
/// wait for a full block.
const EXEC_READ_BYTES: u32 = 4 * 1024 * 1024;

/// The element an exec child's stdout flows out of.
///
/// Deliberately no timestamping. The process writes a container, and stamping
/// buffers with their arrival time before the demuxer sees them destroys the
/// timing the container carries. The demuxer's own timestamps are the correct
/// ones, and the mixer pad offset aligns them afterwards.
#[cfg(unix)]
fn new_exec_source(id: &str) -> Result<gst::Element> {
    let src = make("fdsrc", &format!("{id}-src-exec"))?;
    src.set_property("blocksize", EXEC_READ_BYTES);
    Ok(src)
}

#[cfg(not(unix))]
fn new_exec_source(id: &str) -> Result<gst::Element> {
    let src = gstreamer_app::AppSrc::builder()
        .name(format!("{id}-src-exec"))
        .format(gst::Format::Bytes)
        .stream_type(gstreamer_app::AppStreamType::Stream)
        .block(true)
        .max_bytes(4 * EXEC_READ_BYTES as u64)
        .build();
    Ok(src.upcast())
}

/// Connect a freshly started child's stdout to the source element.
///
/// Called when the source is built and again on every restart, when the old
/// child is gone and a new one has been started: the element stays, the pipe
/// behind it changes.
/// Returns the descriptor to be held for as long as the element reads it. See
/// `ExecStdout`.
#[cfg(unix)]
fn attach_exec_stdout(_id: &str, src: &gst::Element, out: ExecStdout) -> ExecStdoutHeld {
    use std::os::fd::AsRawFd;
    let ExecStdout::Fd(fd) = out;
    src.set_property("fd", fd.as_raw_fd());
    Some(fd)
}

#[cfg(not(unix))]
fn attach_exec_stdout(id: &str, src: &gst::Element, out: ExecStdout) -> ExecStdoutHeld {
    use std::collections::HashMap;
    use std::io::Read;
    // One reader per element at a time. A restart starts a new child and a
    // new reader while the old reader may still be draining the old pipe; the
    // old one must not push, and above all must not end the stream, into the
    // element the new one now owns. Each reader holds a token that the next
    // attach revokes.
    static READERS: std::sync::OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
        std::sync::OnceLock::new();
    let ExecStdout::Pipe(mut pipe) = out;
    let Ok(appsrc) = src.clone().downcast::<gstreamer_app::AppSrc>() else {
        warn!(source = %id, "exec source is not an appsrc; stdout not attached");
        return;
    };
    let token = Arc::new(AtomicBool::new(true));
    {
        let mut readers = READERS.get_or_init(|| Mutex::new(HashMap::new())).lock();
        if let Some(old) = readers.insert(src.name().to_string(), token.clone()) {
            old.store(false, Ordering::SeqCst);
        }
    }
    let id = id.to_string();
    std::thread::Builder::new()
        .name(format!("exec-stdout-{id}"))
        .spawn(move || {
            let mut buf = vec![0u8; EXEC_READ_BYTES as usize];
            loop {
                let n = match pipe.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                if !token.load(Ordering::SeqCst) {
                    return;
                }
                let buffer = gst::Buffer::from_slice(buf[..n].to_vec());
                if appsrc.push_buffer(buffer).is_err() {
                    // Flushing: the pipeline is stopping or restarting. The
                    // child will be killed and this pipe will end.
                    break;
                }
            }
            if token.load(Ordering::SeqCst) {
                let _ = appsrc.end_of_stream();
            }
        })
        .ok();
}

fn make_exec_source(id: &str, spec: &ExecSpec) -> Result<(gst::Element, ExecChild)> {
    let (out, child, stderr) = spawn_exec(id, spec)?;
    let src = new_exec_source(id)?;
    let stdout = attach_exec_stdout(id, &src, out);
    Ok((src, ExecChild { child, stdout, stderr }))
}

/// Build a headless browser rendering a page as a live source.
///
/// `wpesrc` runs a WPE WebKit instance offscreen and exposes what it draws and
/// plays as pads. It is packaged on Linux (`gstreamer1.0-wpe` on Debian and
/// Ubuntu, `gst-plugins-bad` with `wpewebkit` elsewhere) and is not available
/// on macOS, so say plainly what is missing rather than failing obscurely.
fn make_web_source(id: &str, uri: &str) -> Result<gst::Element> {
    let url = web_url(uri).context("not a web source url")?;
    anyhow::ensure!(
        crate::probe::exists("wpesrc"),
        "cannot render web pages: the GStreamer `wpesrc` element is not installed. \
         Install gstreamer1.0-wpe (Debian, Ubuntu) or gst-plugins-bad built with \
         wpewebkit. It is not available on macOS."
    );

    let src = make("wpesrc", &format!("{id}-src-web"))?;
    src.set_property("location", &url);
    // Render onto an opaque page rather than compositing the browser's
    // transparency into the programme.
    crate::probe::set_bool(&src, "draw-background", true);
    info!(source = %id, %url, "rendering web page as a source");
    Ok(src)
}

/// Ask the browser to render at the canvas size and rate.
///
/// Without this it renders at its own default and the normaliser scales up,
/// which is exactly the wrong way round: text and UI come out soft. Rendering
/// at the target size means the page is drawn sharp at full resolution.
fn web_render_caps(canvas: &CanvasCaps) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("width", canvas.width)
        .field("height", canvas.height)
        .field("framerate", canvas.fps)
        .build()
}

/// Build an RTMP source element, tolerating the two implementations having
/// different property sets.
fn make_rtmp_source(element: &str, id: &str, uri: &str) -> Result<gst::Element> {
    let src = make(element, &format!("{id}-src-{element}"))?;
    src.set_property("location", uri);
    // Reconnect handling is ours, not the element's: we control the backoff and
    // report state to the operator. These properties exist on one
    // implementation or the other, never both, so set them defensively.
    crate::probe::set_bool(&src, "async-connect", true);
    crate::probe::set_bool(&src, "no-eof-is-error", true);
    crate::probe::set_bool(&src, "do-timestamp", true);
    Ok(src)
}

fn install_buffer_probe<F>(element: &gst::Element, pad: &str, f: F) -> Result<()>
where
    F: Fn() + Send + Sync + 'static,
{
    let pad = element
        .static_pad(pad)
        .with_context(|| format!("{} has no {pad} pad", element.name()))?;
    pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
        f();
        gst::PadProbeReturn::Ok
    })
    .context("installing buffer probe")?;
    Ok(())
}

/// `livesync` absorbs the drift between an independent encoder's clock and
/// ours, emitting a gapless stream by duplicating or dropping frames. It ships
/// with gst-plugins-rs; when that is not installed we fall through to the
/// videorate and audioresample already in the chain, which is less capable but
/// keeps the mixer usable.
fn optional_livesync(name: &str) -> Result<Option<gst::Element>> {
    if !crate::probe::exists("livesync") {
        warn!("livesync is not installed; clock drift between sources will be handled less well");
        return Ok(None);
    }
    let el = make("livesync", name)?;
    // A single continuous segment is what an aggregator downstream wants.
    crate::probe::set_bool(&el, "single-segment", true);
    // `sync` is what makes livesync time its output to the pipeline clock, and
    // it defaults to true. Turning it off leaves buffers carrying the RTMP
    // stream's own timeline, which starts at zero when the camera connected.
    // A compositor tolerates that by reusing the last frame it holds, so video
    // looks correct; an audiomixer cannot place samples whose position it
    // cannot trust, so the programme carried silence for every camera.
    crate::probe::set_bool(&el, "sync", true);
    Ok(Some(el))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Accel, Canvas, RtmpClient, Superimpose};

    fn init() {
        let _ = gst::init();
    }

    fn test_source() -> SourceConfig {
        SourceConfig {
            id: "cam1".into(),
            name: Some("Camera 1".into()),
            uri: "rtmp://127.0.0.1/live/cam1".into(),
            stall_timeout_secs: 2.0,
            rtmp_client: RtmpClient::Auto,
            superimpose: Superimpose::Off,
            gain: 1.0,
            muted: false,
        }
    }

    #[test]
    fn builds_a_complete_input_pipeline_on_whatever_backend_is_here() {
        init();
        let canvas = CanvasCaps::new(&Canvas::default());
        let backends = Backends::probe(Accel::Auto, Accel::Auto).unwrap();
        let input =
            InputPipeline::build(&test_source(), &canvas, &backends, 15, Instant::now()).unwrap();

        // Proxy sinks must exist before any media arrives, otherwise the mixer
        // could not allocate its pads until a camera connected.
        assert_eq!(input.video_proxy.factory().unwrap().name(), "proxysink");
        assert_eq!(input.thumb_proxy.factory().unwrap().name(), "proxysink");
        assert_eq!(input.audio_proxy.factory().unwrap().name(), "proxysink");

        assert!(!input.has_video());
        assert!(!input.has_audio());
        assert_eq!(input.observed_state(), SourceState::Connecting);
        input.stop();
    }

    /// Descriptors open in this process right now. `/dev/fd` on a Mac and
    /// `/proc/self/fd` on Linux both list exactly them.
    #[cfg(unix)]
    fn open_fds() -> usize {
        let dir = if std::path::Path::new("/proc/self/fd").exists() {
            "/proc/self/fd"
        } else {
            "/dev/fd"
        };
        std::fs::read_dir(dir).map(|d| d.count()).unwrap_or(0)
    }

    /// A source that comes and goes must leave nothing open.
    ///
    /// It left two descriptors a build on air: the read end of the child's
    /// stdout, which was given to fdsrc with into_raw_fd and never closed
    /// again, and the read end of its stderr, held by a reader thread that
    /// never saw an end of file. Counted here over twenty builds, after five
    /// to let the plugin registry and the decoder load whatever they load
    /// once.
    #[test]
    #[cfg(unix)]
    fn a_source_that_comes_and_goes_leaves_no_descriptors_behind() {
        init();
        let canvas = CanvasCaps::new(&Canvas::default());
        let backends = Backends::probe(Accel::Auto, Accel::Auto).unwrap();
        let browser = BrowserConfig::default();
        let mut cfg = test_source();
        // An exec source, because it is the kind with a process, a pipe and a
        // reader thread behind it, which is where all of this went wrong.
        cfg.uri = "exec:sh -c 'exec cat /dev/zero'".into();

        let mut baseline = 0usize;
        for i in 0..20 {
            let input = InputPipeline::build_kind(
                &cfg,
                &canvas,
                &backends,
                8,
                Instant::now(),
                SourceKind::Exec,
                true,
                &browser,
                None,
            )
            .unwrap();
            input.stop();
            drop(input);
            if i == 4 {
                baseline = open_fds();
            }
        }
        let after = open_fds();
        assert!(
            after <= baseline + 2,
            "fifteen builds added {} descriptors ({baseline} to {after})",
            after.saturating_sub(baseline)
        );
    }

    #[test]
    fn uris_are_routed_to_the_right_kind() {
        use SourceKind::*;
        assert_eq!(SourceKind::detect("rtmp://host/live/cam"), Rtmp);
        assert_eq!(SourceKind::detect("RTMPS://host/live/cam"), Rtmp);
        // HLS and DASH are continuous even though they arrive over HTTP.
        assert_eq!(SourceKind::detect("http://host/stream.m3u8"), Live);
        assert_eq!(SourceKind::detect("https://host/a/b.m3u8?token=x"), Live);
        assert_eq!(SourceKind::detect("https://host/manifest.mpd"), Live);
        assert_eq!(SourceKind::detect("rtsp://host/stream"), Live);
        assert_eq!(SourceKind::detect("srt://host:9000"), Live);
        // A plain file, local or fetched, is finite.
        assert_eq!(SourceKind::detect("/srv/ads/spot.mp4"), File);
        assert_eq!(SourceKind::detect("file:///srv/ads/spot.mp4"), File);
        assert_eq!(SourceKind::detect("https://host/spot.mp4"), File);

        assert!(Rtmp.is_continuous() && Live.is_continuous());
        assert!(!File.is_continuous());
    }

    #[test]
    fn urls_are_marked_as_pages_without_doubling_the_prefix() {
        assert_eq!(as_web_uri("https://a.tv/x"), "web+https://a.tv/x");
        assert_eq!(as_web_uri("web+https://a.tv/x"), "web+https://a.tv/x");
        assert_eq!(as_web_uri("web://a.tv"), "web://a.tv");
        assert_eq!(as_web_uri("a.tv/live"), "web+https://a.tv/live");
    }

    #[test]
    fn web_urls_are_recognised_and_unwrapped() {
        assert_eq!(web_url("web+https://example.com/game"), Some("https://example.com/game".into()));
        assert_eq!(web_url("web+http://example.com/x"), Some("http://example.com/x".into()));
        // Bare web:// is shorthand for https.
        assert_eq!(web_url("web://example.com/x"), Some("https://example.com/x".into()));
        assert_eq!(web_url("WEB://Example.com/X"), Some("https://Example.com/X".into()));
        // Everything else is left alone.
        assert_eq!(web_url("https://example.com/x"), None);
        assert_eq!(web_url("rtmp://host/live/x"), None);
        assert_eq!(web_url("/srv/ads/clip.mp4"), None);

        assert_eq!(SourceKind::detect("web+https://example.com/game"), SourceKind::Web);
        assert_eq!(SourceKind::detect("web://example.com"), SourceKind::Web);
        // A web page is continuous, so it is re-timed and restarted like a camera.
        assert!(SourceKind::Web.is_continuous());
        // Without the marker the same URL is not treated as a page.
        assert_ne!(SourceKind::detect("https://example.com/game"), SourceKind::Web);
    }

    /// The renderer is Linux-only. Where it is missing, adding a web source has
    /// to say what to install rather than failing somewhere obscure later.
    #[test]
    fn a_missing_web_renderer_explains_itself() {
        init();
        if crate::probe::exists("wpesrc") {
            // Present here, so there is nothing to assert about its absence.
            return;
        }
        let err = make_web_source("s", "web+https://example.com").unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("wpesrc"), "should name the element: {msg}");
        assert!(msg.contains("gstreamer1.0-wpe"), "should name the package: {msg}");
    }

    /// The sidecar's report is the whole basis for taking the layered path, so
    /// a line that says the media cannot be opened must not be read as one that
    /// says it can.
    #[test]
    fn media_reports_are_read_off_the_sidecars_stderr() {
        let usable = r#"[browser] media {"found":true,"count":1,"tag":"video","src":"http://h/v.webm","usable":true,"mse":false,"drm":false,"paused":false,"rect":{"x":0,"y":0,"w":640,"h":360},"intrinsic":{"w":1280,"h":720},"viewport":{"w":1280,"h":720}}"#;
        let r = media_report(usable).expect("a real report parses");
        assert!(r.usable);
        assert_eq!(r.src, "http://h/v.webm");

        // A page feeding its own player has nothing to hand over.
        let mse = r#"[browser] media {"found":true,"count":1,"src":"blob:https://x/1","usable":false,"mse":true}"#;
        assert!(!media_report(mse).expect("an mse report parses").usable);
        // And a page with no media at all reports that much.
        assert!(!media_report(r#"[browser] media {"found":false,"count":0}"#).unwrap().usable);

        // Anything else on stderr is the browser's own noise.
        assert!(media_report("[browser] painted 30 frames").is_none());
        assert!(media_report("[browser] media not json").is_none());
    }

    /// The page measures its video in viewport pixels; the canvas may be a
    /// different size and the picture still has to land where the page had it.
    #[test]
    fn page_geometry_is_scaled_onto_the_canvas() {
        let canvas = CanvasCaps::new(&Canvas::default()); // 1920x1080
        let report = |x, y, w, h, vw, vh| MediaReport {
            found: true,
            mse: false,
            drm: false,
            media: vec![],
            src: "http://h/v".into(),
            usable: true,
            rect: MediaRect { x, y, w, h },
            viewport: MediaSize { w: vw, h: vh },
        };

        // A video filling its viewport fills the canvas, whatever size the
        // page was rendered at.
        assert_eq!(report(0, 0, 1280, 720, 1280, 720).placement(&canvas), (0, 0, 1920, 1080));
        assert_eq!(report(0, 0, 1920, 1080, 1920, 1080).placement(&canvas), (0, 0, 1920, 1080));
        // A quarter of the viewport, in its middle, is a quarter of the canvas.
        assert_eq!(report(160, 90, 320, 180, 640, 360).placement(&canvas), (480, 270, 960, 540));
        // A rectangle from a page still laying itself out is not usable
        // geometry, so the video takes the whole canvas rather than vanishing.
        assert_eq!(report(0, 0, 0, 0, 1280, 720).placement(&canvas), (0, 0, 1920, 1080));
        assert_eq!(report(0, 0, 640, 360, 0, 0).placement(&canvas), (0, 0, 1920, 1080));
    }

    /// End to end against a stand-in sidecar: the probe must return on the
    /// first usable report rather than on its timeout, and it must leave
    /// nothing running behind it.
    #[test]
    fn the_probe_returns_on_the_first_usable_report() {
        let spec = ExecSpec::from_uri(
            "exec:sh -c 'echo one >&2; \
             echo \"[browser] media {\\\"found\\\":false}\" >&2; \
             echo \"[browser] media {\\\"src\\\":\\\"http://h/v.m3u8\\\",\\\"usable\\\":true}\" >&2; \
             while :; do echo data; sleep 0.1; done'",
            true,
        )
        .unwrap();

        let started = Instant::now();
        let report = probe_page_media(&"s".to_string(), &spec, Duration::from_secs(10))
            .expect("the usable report should be taken");
        assert_eq!(report.src, "http://h/v.m3u8");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "should return on the report, not the timeout: {:?}",
            started.elapsed()
        );

        // A sidecar that says nothing gives up at the timeout, and the source
        // is then built exactly as it is today.
        let quiet = ExecSpec::from_uri("exec:sh -c 'sleep 30'", true).unwrap();
        assert!(probe_page_media(&"s".to_string(), &quiet, Duration::from_secs(1)).is_none());
    }

    /// The probe runs a sidecar per source build and reads its report off a
    /// pipe. It must give the pipe back: this is the other half of the two
    /// descriptors a build leaked on air.
    #[test]
    #[cfg(unix)]
    fn probing_a_page_leaves_no_descriptors_behind() {
        let spec = ExecSpec::from_uri(
            "exec:sh -c 'echo \"[browser] media {\\\"src\\\":\\\"http://h/v.m3u8\\\",\\\"usable\\\":true}\" >&2; \
             while :; do sleep 0.1; done'",
            true,
        )
        .unwrap();
        let mut baseline = 0usize;
        for i in 0..12 {
            assert!(probe_page_media(&"s".to_string(), &spec, Duration::from_secs(5)).is_some());
            if i == 2 {
                baseline = open_fds();
            }
        }
        let after = open_fds();
        assert!(
            after <= baseline + 2,
            "nine probes added {} descriptors ({baseline} to {after})",
            after.saturating_sub(baseline)
        );
    }

    #[test]
    fn exec_commands_are_recognised_and_unwrapped() {
        assert_eq!(exec_command("exec:ffmpeg -i x -f mpegts -"), Some("ffmpeg -i x -f mpegts -"));
        assert_eq!(exec_command("exec://ffmpeg -i x"), Some("ffmpeg -i x"));
        assert_eq!(exec_command("EXEC:  ffmpeg -i x  "), Some("ffmpeg -i x"));
        assert_eq!(exec_command("rtmp://host/live/x"), None);
        assert_eq!(exec_command("/srv/clip.mp4"), None);
        assert_eq!(SourceKind::detect("exec:ffmpeg -i x"), SourceKind::Exec);
        // Restarted like any other continuous source, so a command that exits
        // is simply run again.
        assert!(SourceKind::Exec.is_continuous());
        assert!(!SourceKind::Exec.wants_livesync(), "a process paces itself");
        assert!(SourceKind::Rtmp.wants_livesync());
    }

    /// Running a command line is a much bigger grant than switching cameras,
    /// so it must stay shut unless the operator opened it deliberately.
    #[test]
    fn a_page_runs_through_the_sidecar_when_one_is_configured() {
        let dir = std::env::temp_dir().join(format!("lbx-sidecar-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("liveboxmix-browser");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let canvas = CanvasCaps::new(&crate::config::Canvas::default());
        let mut browser = BrowserConfig {
            sidecar: Some(bin.to_string_lossy().to_string()),
            args: vec!["--verbose".into()],
            ..Default::default()
        };
        browser.env.insert("PULSE_SINK".into(), "lbx".into());

        let spec = ExecSpec::browser("web+https://example.com/x?a=1 b", &canvas, &browser)
            .unwrap()
            .expect("a configured sidecar is used");
        assert_eq!(spec.argv[0], bin.to_string_lossy());
        assert_eq!(&spec.argv[1..3], ["--url", "https://example.com/x?a=1 b"]);
        assert!(spec.argv.contains(&"--width".to_string()));
        assert_eq!(spec.argv.last().unwrap(), "--verbose");
        assert_eq!(spec.env.get("PULSE_SINK").unwrap(), "lbx");

        // A configured path that is missing is an error, not a silent fallback.
        browser.sidecar = Some(dir.join("nope").to_string_lossy().to_string());
        assert!(ExecSpec::browser("web://example.com", &canvas, &browser).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A superimposed page draws chrome, not video, and its frames cross to the
    /// mixer raw with an alpha channel at 4 bytes a pixel. At the canvas rate a
    /// 720p page is 110 MB/s, measured, where an ordinary I420 source is 41.
    /// Dropping the page to `overlay_fps` puts it at 37 MB/s, under what the
    /// pipe already carries every day. Getting this wrong does not fail a test
    /// anywhere else, it just makes the feature cost more than it saves.
    #[test]
    fn a_superimposed_page_draws_slower_than_the_canvas() {
        let canvas = CanvasCaps::new(&crate::config::Canvas::default());
        let browser = BrowserConfig {
            // Appended after the built argv, and the sidecar takes the last
            // spelling of a flag, so this must keep beating the overlay rate.
            args: vec!["--fps".into(), "60".into()],
            ..Default::default()
        };
        let mut spec = ExecSpec {
            argv: vec![
                "liveboxmix-browser".into(),
                "--fps".into(),
                canvas.fps.numer().to_string(),
            ],
            env: browser.env.clone(),
        };
        spec.argv.extend(browser.args.iter().cloned());

        spec.set_fps(default_overlay_fps_for_test());
        let fps: Vec<&String> = spec
            .argv
            .iter()
            .enumerate()
            .filter(|(i, _)| *i > 0 && spec.argv[i - 1] == "--fps")
            .map(|(_, v)| v)
            .collect();
        assert_eq!(fps, ["10", "60"], "ours is lowered, the operator's still wins");
    }

    /// Kept next to the test so a change to the default is noticed here.
    fn default_overlay_fps_for_test() -> u32 {
        BrowserConfig::default().overlay_fps
    }

    #[test]
    fn exec_sources_are_refused_unless_enabled() {
        let err = ExecSpec::from_uri("exec:echo hi", false).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("disabled"), "should say it is off: {msg}");
        assert!(msg.contains("allow_exec_sources"), "should name the setting: {msg}");

        // And an empty command is refused even when enabled.
        assert!(ExecSpec::from_uri("exec:", true).is_err());
    }

    /// The sidecar names its profile from its own pid and removes it when its
    /// message loop ends, which a killed sidecar never reaches. 1084 of them
    /// were left in a container's /tmp on 2026-09-12, 18 GB of it.
    #[test]
    fn a_killed_sidecars_profile_directory_is_taken_with_it() {
        let tmp = std::env::temp_dir().join(format!("lbx-profile-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let env: std::collections::BTreeMap<String, String> =
            [("TMPDIR".to_string(), tmp.to_string_lossy().to_string())].into_iter().collect();

        let dir = browser_profile_dir(&env, 4242);
        assert_eq!(dir, tmp.join("lbx-browser-4242"));
        // A profile is a tree, not a file, and CEF leaves it locked open until
        // the process goes; nothing here may assume it is empty.
        std::fs::create_dir_all(dir.join("Default/Cache")).unwrap();
        std::fs::write(dir.join("Default/Cache/data_0"), vec![0u8; 4096]).unwrap();
        remove_browser_profile(&env, 4242);
        assert!(!dir.exists(), "the profile directory should be gone");

        // And doing it again, or for a source that never had one, is quiet.
        remove_browser_profile(&env, 4242);
        remove_browser_profile(&env, 9999);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The write end of a child's stderr belongs to every process that
    /// inherited it, so killing the child it was opened for does not
    /// necessarily close it. Before the poll went in, a reader thread in that
    /// position never returned: a thread and a descriptor gone for the life of
    /// the mixer, every rebuild.
    #[test]
    #[cfg(unix)]
    fn the_stderr_reader_ends_even_when_a_grandchild_holds_the_pipe() {
        // `sh` exits at once and leaves a background process holding stderr.
        let spec = ExecSpec::from_uri(
            "exec:sh -c 'sleep 30 >/dev/null 2>&1 & echo hello 1>&2; exit 0'",
            true,
        )
        .unwrap();
        let mut child = exec_process(&spec, std::process::Stdio::null()).unwrap();
        let err = child.stderr.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut reader = StderrReader::spawn("test-stderr".into(), err, move |line| {
            let _ = tx.send(line.to_string());
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "hello");
        let _ = child.wait();

        // The background `sleep` still holds the write end, so there is no end
        // of file to wait for. Stopping must return anyway, and quickly.
        let began = Instant::now();
        reader.stop();
        assert!(
            began.elapsed() < Duration::from_secs(2),
            "stopping took {:?}",
            began.elapsed()
        );
        stop_process_group(child.id());
    }

    #[test]
    #[cfg(unix)] // `sh`, and a descriptor to read back; Windows hands a pipe to a reader thread instead
    fn an_enabled_exec_source_starts_its_process() {
        let spec = ExecSpec::from_uri("exec:sh -c 'printf hello; sleep 5'", true).unwrap();
        let (ExecStdout::Fd(fd), mut child, _err) = spawn_exec("s", &spec).unwrap();
        use std::os::fd::AsRawFd;
        assert!(fd.as_raw_fd() > 2, "should hand back a real pipe descriptor");
        // The element reads this descriptor but does not own it; read it back
        // here to prove it is live, without taking it away.
        use std::io::Read;
        let mut f = std::fs::File::from(fd);
        let mut buf = [0u8; 5];
        f.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn restarts_do_not_stack_up() {
        init();
        let canvas = CanvasCaps::new(&Canvas::default());
        let backends = Backends::probe(Accel::Auto, Accel::Auto).unwrap();
        let input =
            InputPipeline::build(&test_source(), &canvas, &backends, 15, Instant::now()).unwrap();

        // A source whose server has died emits a burst of errors. Only the
        // first may arm a restart: fifty restarts in ten milliseconds is
        // enough to kill the process.
        assert!(input.try_arm_restart(), "first error should arm a restart");
        for _ in 0..20 {
            assert!(!input.try_arm_restart(), "a burst of errors must arm only one restart");
        }
        input.restart().unwrap();
        assert!(input.try_arm_restart(), "a later failure may arm again");
        input.stop();
    }

    #[test]
    fn the_client_fallback_fires_once_and_only_in_auto_mode() {
        init();
        let canvas = CanvasCaps::new(&Canvas::default());
        let backends = Backends::probe(Accel::Auto, Accel::Auto).unwrap();

        let input =
            InputPipeline::build(&test_source(), &canvas, &backends, 15, Instant::now()).unwrap();
        assert!(input.never_connected());

        // Whether the replacement pipeline can reach PLAYING depends on there
        // being a server at the other end, which a unit test must not require.
        // The invariant being asserted is that the swap is one-shot.
        let _ = input.try_fallback_client();
        assert!(
            input.fallback_used.load(Ordering::SeqCst),
            "the first attempt should spend the one-shot swap"
        );
        assert!(!input.try_fallback_client().unwrap(), "it must not swap repeatedly");
        input.stop();

        // A pinned client is the operator's decision; never second-guess it.
        let mut pinned = test_source();
        pinned.rtmp_client = RtmpClient::Rtmp2;
        let input = InputPipeline::build(&pinned, &canvas, &backends, 15, Instant::now()).unwrap();
        assert!(!input.try_fallback_client().unwrap(), "a pinned client must not be swapped");
        input.stop();
    }

    #[test]
    fn a_source_that_never_connects_reads_as_connecting_not_stalled() {
        init();
        let canvas = CanvasCaps::new(&Canvas::default());
        let backends = Backends::probe(Accel::Auto, Accel::Auto).unwrap();
        let mut cfg = test_source();
        cfg.stall_timeout_secs = 0.0;
        let input = InputPipeline::build(&cfg, &canvas, &backends, 15, Instant::now()).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        assert_eq!(input.observed_state(), SourceState::Connecting);

        // Once media has been seen, silence becomes a stall.
        input.health.mark_video();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert_eq!(input.observed_state(), SourceState::Stalled);

        input.mark_failed();
        assert_eq!(input.observed_state(), SourceState::Failed);
        input.stop();
    }

    /// The real elements, not a stand in. Reading a gain back off a `volume`
    /// element is the whole point of `report`, so a test that modelled the
    /// gains in Rust would prove nothing about what the pipeline is doing.
    fn test_levels(videos: usize) -> AudioLevels {
        init();
        AudioLevels {
            page: make("volume", "test-page-vol").unwrap(),
            media: (0..videos)
                .map(|n| make("volume", &format!("test-media{n}-vol")).unwrap())
                .collect(),
        }
    }

    #[test]
    fn gains_are_clamped_to_what_a_volume_element_accepts() {
        let levels = test_levels(2);
        // A fader dragged past the end of its track still moves the sound.
        levels.set_page(-3.0);
        assert_eq!(levels.page_gain(), 0.0);
        levels.set_page(99.0);
        assert_eq!(levels.page_gain(), 10.0);
        assert!(levels.set_media(1, -1.0));
        assert_eq!(levels.media_gains()[1], 0.0);
        // An index with no video behind it says so rather than passing.
        assert!(!levels.set_media(9, 0.5));
        assert_eq!(levels.media_count(), 2);
    }

    /// The UI sends one fader at a time. A page gain arriving on its own used
    /// to be the moment every video would snap back to unity if `apply` wrote
    /// a whole balance instead of the part it was given.
    #[test]
    fn a_partial_balance_leaves_the_other_channels_alone() {
        let levels = test_levels(2);
        let all = levels.apply(Some(0.5), &[Some(0.25), Some(0.75)]);
        assert_eq!(all, SourceAudio { page: 0.5, media: vec![0.25, 0.75] });

        let page_only = levels.apply(Some(0.125), &[]);
        assert_eq!(page_only, SourceAudio { page: 0.125, media: vec![0.25, 0.75] });

        // A list shorter than the number of videos stops where it stops.
        let first_only = levels.apply(None, &[Some(1.0)]);
        assert_eq!(first_only, SourceAudio { page: 0.125, media: vec![1.0, 0.75] });

        // A null holds that channel. This is how the UI moves the second
        // video without restating the first, which a short list cannot do.
        let second_only = levels.apply(None, &[None, Some(0.25)]);
        assert_eq!(second_only, SourceAudio { page: 0.125, media: vec![1.0, 0.25] });

        // A list longer than the number of videos is not an error, and the
        // report says how many there really are.
        let too_many = levels.apply(None, &[Some(0.5), Some(0.5), Some(0.5), Some(0.5)]);
        assert_eq!(too_many, SourceAudio { page: 0.125, media: vec![0.5, 0.5] });

        // An empty request reads the balance without moving anything.
        assert_eq!(levels.apply(None, &[]), too_many);
    }

    /// A whole page source has no levels at all: Chromium mixed its sounds
    /// together long before the mixer saw them, and pretending otherwise
    /// would give the operator faders that move nothing.
    #[test]
    fn a_plain_source_reports_no_levels() {
        init();
        let canvas = CanvasCaps::new(&Canvas::default());
        let backends = Backends::probe(Accel::Auto, Accel::Auto).unwrap();
        let input =
            InputPipeline::build(&test_source(), &canvas, &backends, 15, Instant::now()).unwrap();
        assert!(!input.superimposed());
        assert!(input.levels().is_none());
        input.stop();
    }
}
