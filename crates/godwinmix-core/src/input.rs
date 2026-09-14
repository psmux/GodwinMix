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
use crate::plugin::kinds::layered::{cache_media, Fetched, LayerCounts, Placement};
pub use crate::plugin::kinds::layered::AudioLevels;
use crate::config::{BrowserConfig, SourceConfig, Superimpose};
use crate::gstutil::{self, make};
use crate::probe::Backends;
use crate::state::{SourceHealth, SourceId, SourceState};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
        // One table, not two. The prefix rules that used to be written out here
        // live in the kinds now, one `claims` function each, and the highest
        // rank wins; this maps the winner back onto the enum the control plane
        // and the ad break still speak. Anything a plugin adds that this build
        // has never heard of reads as a file, which is what an unrecognised URI
        // always read as.
        match crate::plugin::source::resolve(uri).map(|p| p.manifest.plugin) {
            Some("exec") => Self::Exec,
            Some("browser") | Some("layered") => Self::Web,
            Some("rtmp") => Self::Rtmp,
            Some("hls") | Some("test") => Self::Live,
            _ => Self::File,
        }
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

/// The mixer's handle on one running source.
///
/// It owns the pipeline the source built, the proxy sinks the programme and
/// the multiview attach to, and the `Source` implementation itself. Everything
/// kind specific lives behind that box: this type holds what every source has.
pub struct InputPipeline {
    pub id: SourceId,
    pub config: SourceConfig,
    pub pipeline: gst::Pipeline,
    pub health: Arc<SourceHealth>,
    /// Where this source's last picture and last sound sat on the timeline as
    /// they left for the programme. See `LastBuffer`.
    pub last_video: Arc<LastBuffer>,
    pub last_audio: Arc<LastBuffer>,
    /// Proxy sinks the program and multiview pipelines attach to. They are
    /// created up front, before any media has arrived, so that the mixer can
    /// allocate its pads without waiting for a camera to connect.
    pub video_proxy: gst::Element,
    pub audio_proxy: gst::Element,
    /// The thumbnail proxy, when a thumbnail end was built. Behind a lock
    /// because it can be attached and detached while the source runs.
    thumb_proxy: Mutex<Option<gst::Element>>,
    /// The canvas capsfilters, which are the per source filter insertion
    /// points, and the tee a thumbnail end hangs off.
    vcaps: gst::Element,
    acaps: gst::Element,
    vtee: gst::Element,
    has_video: Arc<AtomicBool>,
    has_audio: Arc<AtomicBool>,
    /// Whether the page's media ended up being decoded outside the browser.
    /// Settled when the pipeline is built and constant for its lifetime.
    superimposed: bool,
    /// Present only for a superimposed source, which is the only kind whose
    /// sounds arrive separately enough to be balanced. See `AudioLevels`.
    levels: Option<AudioLevels>,
    /// What each side of a layered source's compositor has done. Only a
    /// layered source has one. See `LayerCounts`.
    counts: Option<Arc<LayerCounts>>,
    /// Where a layered source's layers sit in time, one per layer. Reset on
    /// restart.
    placement: Vec<Arc<Placement>>,
    /// Set when the pipeline posts an error; the supervisor restarts it.
    failed: Arc<AtomicBool>,
    /// Whether this pipeline can be scrubbed, once it has said. `None` until
    /// then, because nothing upstream answers a SEEKING query before the chain
    /// from the source to the proxies is built, and a query nobody answered is
    /// not the same as a no.
    seekable: Mutex<Option<bool>>,
    /// Set while a restart is pending. A source whose server has gone away
    /// emits a burst of bus errors, and without this each one arms its own
    /// restart. They then all fire together, tearing the pipeline down and
    /// rebuilding it dozens of times in a few milliseconds, which is enough to
    /// take the whole process down.
    restart_armed: AtomicBool,
    /// What this source said about itself at `initialize`, and what the
    /// supervisor is allowed to assume from it.
    manifest: crate::plugin::Manifest,
    capabilities: crate::plugin::CapabilitySet,
    /// The implementation. A child process, an RTMP client element, a pair of
    /// decoders: whatever it is, it is in here and nothing else sees it.
    kind: Mutex<Box<dyn crate::plugin::Source>>,
    /// Filters inserted per source, on the input side of the proxy boundary.
    filters: Mutex<Vec<crate::plugin::FilterSlot>>,
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
    pub fn set_fps(&mut self, fps: u32) {
        if let Some(i) = self.argv.iter().position(|a| a == "--fps") {
            if let Some(v) = self.argv.get_mut(i + 1) {
                *v = fps.to_string();
            }
        }
    }
}

/// Where `godwinmix-browser` is: the configured path, else next to this
/// executable, else on PATH.
fn find_browser_sidecar(browser: &BrowserConfig) -> Result<Option<std::path::PathBuf>> {
    const NAME: &str = "godwinmix-browser";
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
    pub found: bool,
    /// The headline element, the one a viewer would call "the video". These
    /// fields are what a sidecar from before `media` existed reports, and they
    /// are still filled in for it.
    #[serde(default)]
    pub src: String,
    #[serde(default)]
    pub usable: bool,
    #[serde(default)]
    pub mse: bool,
    #[serde(default)]
    pub drm: bool,
    #[serde(default)]
    pub rect: MediaRect,
    #[serde(default)]
    pub viewport: MediaSize,
    /// Every video on the page, in document order. After the probe has run
    /// this holds only the ones the mixer will draw itself.
    #[serde(default)]
    pub media: Vec<MediaItem>,
}

/// One video element on the page.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MediaItem {
    /// Position in document order, which is also its place in the stacking.
    #[serde(default)]
    pub index: usize,
    /// The address a decoder outside the browser can open.
    #[serde(default)]
    pub src: String,
    /// The sidecar's own verdict on that address. False for a `blob:` URL fed
    /// by JavaScript, for DRM, and for an element with no source yet.
    #[serde(default)]
    pub usable: bool,
    /// The element is fed from JavaScript through Media Source Extensions, so
    /// its address is a `blob:` that exists only inside that renderer.
    #[serde(default)]
    pub mse: bool,
    /// Encrypted Media Extensions: decrypted inside the browser, never leaves.
    #[serde(default)]
    pub drm: bool,
    /// The page plays this one silently. The mixer's copy is muted to match.
    #[serde(default)]
    pub muted: bool,
    /// Where the element sat in the page, in CSS pixels.
    #[serde(default)]
    pub rect: MediaRect,
    /// The size of the viewport that rectangle was measured in.
    #[serde(default)]
    pub viewport: MediaSize,
    /// A local copy of a finite clip, when one was fetched. See `cache_media`.
    /// Deleted when the source stops.
    #[serde(skip)]
    pub cache: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct MediaRect {
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct MediaSize {
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
}

impl MediaReport {
    /// Whether anything on the page can be handed over.
    pub fn any_usable(&self) -> bool {
        self.usable || self.media.iter().any(|m| m.usable)
    }

    /// The videos in the report, as items. A report from a sidecar that only
    /// knew about one video has an empty `media` list and its headline fields
    /// carry that one, so it becomes the single item.
    pub fn items(&self) -> Vec<MediaItem> {
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
    pub fn placement(&self, canvas: &CanvasCaps) -> (i32, i32, i32, i32) {
        placement_of(self.rect, self.viewport, canvas)
    }
}

impl MediaItem {
    /// Where this video goes on the canvas.
    pub fn placement(&self, canvas: &CanvasCaps) -> (i32, i32, i32, i32) {
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
        bury_child(child, spec.env.clone());
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

    // The reader's thread and descriptor go first, so that the sidecar sees
    // the far end of its stderr close and stops for that reason too; then the
    // process itself, killed and waited for on a thread of its own so the
    // probe returns at once. The clip fetch below is what the caller is
    // waiting for and it does not need a dead browser.
    reader.stop();
    bury_child(child, spec.env.clone());
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
            false,
            &BrowserConfig::default(),
            None,
            true,
        )
    }

    /// Pick the source implementation this config names and build it.
    ///
    /// This was 386 lines multiplexing five kinds through optional element
    /// slots. It is now a dispatcher: the registry says which `Source` opens
    /// this config, the handshake settles what the instance can do, and the
    /// kind builds its own pipeline over the shared normaliser. Nothing here
    /// knows what RTMP or a browser is.
    #[allow(clippy::too_many_arguments)]
    pub fn build_kind(
        cfg: &SourceConfig,
        canvas: &CanvasCaps,
        backends: &Backends,
        thumb_fps: i32,
        origin: Instant,
        allow_exec: bool,
        browser: &BrowserConfig,
        overlay: Option<MediaReport>,
        thumb: bool,
    ) -> Result<Self> {
        // Tags every line logged below with this source's instance, so
        // `log.set {instance, level}` reaches it, and times the build for
        // `--startup-report`. See `observe::source_span`.
        let _observe = crate::observe::source_span(&cfg.id);
        let mut provide = crate::plugin::source::resolve_config(cfg)?;
        // A page that was probed and came back with media to take over is
        // built as layers instead. The substitution is here, in the core,
        // rather than in the browser kind, because it is the core that ran the
        // probe and the core that decides a source is worth two decoders.
        if overlay.is_some() && provide.manifest.plugin == "browser" {
            provide = &crate::plugin::kinds::layered::PROVIDE;
        }
        let request = crate::plugin::source::SourceRequest {
            cfg,
            canvas,
            backends,
            browser,
            allow_exec,
            thumb_fps,
            origin,
            overlay,
        };
        let mut kind = (provide.make)(request)?;
        let ready = kind.initialize(crate::plugin::Hello {
            instance: cfg.id.clone(),
            canvas: canvas.clone(),
            api_level: crate::plugin::API_LEVEL,
            params: cfg.effective_params(),
            tier: crate::plugin::Tier::Core,
        })?;
        let ends = kind.start(canvas, thumb)?;
        Ok(Self::over(cfg, kind, ready, ends))
    }

    /// Wrap a built source's ends in the handle the mixer works with.
    fn over(
        cfg: &SourceConfig,
        kind: Box<dyn crate::plugin::Source>,
        ready: crate::plugin::Ready,
        ends: crate::plugin::MediaEnds,
    ) -> Self {
        let parts = ends.parts;
        Self {
            id: cfg.id.clone(),
            config: cfg.clone(),
            pipeline: ends.pipeline,
            health: ends.health,
            last_video: ends.last_video,
            last_audio: ends.last_audio,
            video_proxy: ends.video,
            audio_proxy: ends.audio,
            thumb_proxy: Mutex::new(ends.thumb),
            vcaps: ends.vcaps,
            acaps: ends.acaps,
            vtee: ends.vtee,
            has_video: parts.has_video.unwrap_or_default(),
            has_audio: parts.has_audio.unwrap_or_default(),
            superimposed: parts.superimposed,
            levels: parts.levels,
            counts: parts.layer_counts,
            placement: parts.placement,
            failed: Arc::new(AtomicBool::new(false)),
            seekable: Mutex::new(None),
            restart_armed: AtomicBool::new(false),
            manifest: ready.manifest,
            capabilities: ready.capabilities,
            kind: Mutex::new(kind),
            filters: Mutex::new(Vec::new()),
        }
    }

    /// What this source is, as a plugin qualified id.
    pub fn type_id(&self) -> String {
        self.manifest.provide_id()
    }

    pub fn manifest(&self) -> &crate::plugin::Manifest {
        &self.manifest
    }

    /// What the supervisor may assume about this instance. Declared by the
    /// kind at `initialize`, not guessed from a flag here.
    pub fn capabilities(&self) -> crate::plugin::CapabilitySet {
        self.capabilities
    }

    /// The kind's own view of itself, for a kind that answers `health`.
    pub fn plugin_health(&self) -> crate::plugin::Health {
        self.kind.lock().health()
    }

    /// Ask the source for something the core does not model: a restart, a
    /// client swap, a tool a plugin contributes.
    pub fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        self.kind.lock().call(method, params)
    }

    /// The thumbnail proxy, when this source has a thumbnail end.
    pub fn thumb_proxy(&self) -> Option<gst::Element> {
        self.thumb_proxy.lock().clone()
    }

    /// Build the thumbnail end on a running source, without a rebuild.
    ///
    /// A tee hands out a new src pad while the others keep flowing, so the
    /// programme branch never sees this: measured as no change at all on the
    /// programme's frame interval, because nothing on that path is touched.
    /// The branch is added in NULL and synced to the parent, which is how
    /// every other element in a live pipeline is added here.
    pub fn attach_thumb_end(&self, canvas: &CanvasCaps, thumb_fps: i32) -> Result<gst::Element> {
        let mut held = self.thumb_proxy.lock();
        if let Some(existing) = held.as_ref() {
            return Ok(existing.clone());
        }
        let id = &self.id;
        let queue = gstutil::queue_thread(&format!("{id}-vthumb-q"))?;
        let rate = make("videorate", &format!("{id}-trate"))?;
        let scale = make("videoscale", &format!("{id}-tscale"))?;
        let caps = gstutil::capsfilter(
            &format!("{id}-tcaps"),
            &CanvasCaps::video_at(
                THUMB_WIDTH,
                THUMB_HEIGHT,
                gst::Fraction::new(thumb_fps.max(1), 1),
            ),
        )?;
        let proxy = make("proxysink", &format!("{id}-tproxy"))?;
        let _ = canvas;
        let branch = [&queue, &rate, &scale, &caps, &proxy];
        self.pipeline.add_many(branch).context("adding a thumbnail end")?;
        gst::Element::link_many([&self.vtee, &queue, &rate, &scale, &caps, &proxy])
            .context("linking a thumbnail end")?;
        for el in branch {
            el.sync_state_with_parent().ok();
        }
        debug!(source = %id, "thumbnail end attached to a running source");
        *held = Some(proxy.clone());
        Ok(proxy)
    }

    /// Take the thumbnail end back out, for a source nobody is looking at.
    pub fn detach_thumb_end(&self) {
        let Some(proxy) = self.thumb_proxy.lock().take() else { return };
        let name = format!("{}-vthumb-q", self.id);
        let Some(queue) = self.pipeline.by_name(&name) else { return };
        let Some(sink) = queue.static_pad("sink") else { return };
        let Some(teepad) = sink.peer() else { return };
        let _ = gstutil::with_pad_blocked(&teepad, std::time::Duration::from_secs(2), || {});
        for part in ["vthumb-q", "trate", "tscale", "tcaps", "tproxy"] {
            if let Some(el) = self.pipeline.by_name(&format!("{}-{part}", self.id)) {
                let _ = el.set_state(gst::State::Null);
                let _ = self.pipeline.remove(&el);
            }
        }
        self.vtee.release_request_pad(&teepad);
        let _ = proxy;
        debug!(source = %self.id, "thumbnail end detached");
    }

    /// Put a filter on this source's input side, between the canvas
    /// capsfilter and the tee.
    ///
    /// Everything downstream of that capsfilter is byte for byte
    /// interchangeable, so this is the one place on a source where a filter can
    /// sit without knowing what kind of source it is. Both the programme and
    /// the thumbnail see it, because both hang off the tee below.
    ///
    /// `live` decides whether the link is moved under a pad block. At build
    /// time it is not, because nothing is flowing yet.
    pub fn attach_filter(
        &self,
        cfg: &crate::config::FilterConfig,
        canvas: &CanvasCaps,
        live: bool,
    ) -> Result<()> {
        anyhow::ensure!(
            !self.filters.lock().iter().any(|f| f.id() == cfg.id),
            "source {} already has a filter called {}",
            self.id,
            cfg.id
        );
        let filter = crate::plugin::filter::make(&cfg.type_id)?;
        let (upstream, downstream) = match cfg.attach.side {
            // The audio insertion point is the audio capsfilter and the proxy
            // below it; the video one is the video capsfilter and the tee.
            _ if filter.stream() == crate::plugin::filter::Stream::Audio => {
                (self.acaps.clone(), self.audio_proxy.clone())
            }
            _ => (self.vcaps.clone(), self.vtee.clone()),
        };
        let mut params = cfg.params.clone();
        params.entry("id".to_string()).or_insert_with(|| {
            toml::Value::String(format!("{}-{}", self.id, cfg.id))
        });
        let slot = crate::plugin::filter::insert(
            crate::plugin::Insertion::between(&self.pipeline, &upstream, &downstream),
            crate::plugin::FilterSpec {
                id: cfg.id.clone(),
                type_id: cfg.type_id.clone(),
                side: crate::plugin::FilterSide::SourceInput,
                params,
            },
            filter,
            canvas,
            live,
        )?;
        self.filters.lock().push(slot);
        Ok(())
    }

    /// Change a filter already on this source.
    pub fn configure_filter(
        &self,
        id: &str,
        params: &crate::config::Params,
    ) -> Result<crate::plugin::Configure> {
        let mut held = self.filters.lock();
        let slot = held
            .iter_mut()
            .find(|f| f.id() == id)
            .with_context(|| format!("source {} has no filter called {id}", self.id))?;
        slot.configure(params)
    }

    /// Take a filter off this source, relinking around it under a pad block.
    pub fn remove_filter(&self, id: &str) -> Result<()> {
        let mut held = self.filters.lock();
        let pos = held
            .iter()
            .position(|f| f.id() == id)
            .with_context(|| format!("source {} has no filter called {id}", self.id))?;
        held.remove(pos).remove()
    }

    /// The filters on this source, in the order they were added.
    pub fn filter_ids(&self) -> Vec<String> {
        self.filters.lock().iter().map(|f| f.id().to_string()).collect()
    }

    /// Each filter's id, type and side, for a listing.
    pub fn filters(&self) -> Vec<(String, String, String)> {
        self.filters
            .lock()
            .iter()
            .map(|f| {
                (f.id().to_string(), f.spec.type_id.clone(), f.spec.side.as_str().to_string())
            })
            .collect()
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
        // The kind releases whatever it holds outside the pipeline: a child
        // process, a cached clip, a profile directory. The core does not know
        // what any of those are.
        if let Err(e) = self.kind.lock().stop() {
            warn!(source = %self.id, ?e, "the source did not stop cleanly");
        }
    }

    pub fn mark_failed(&self) {
        self.failed.store(true, Ordering::Relaxed);
    }

    /// Ask an RTMP source to swap to the other client implementation.
    ///
    /// Returns false for any source that has no such swap to make, which is
    /// every kind but one. Called only for a source that has produced no media
    /// at all, so nothing downstream has state to lose.
    pub fn try_fallback_client(&self) -> Result<bool> {
        let swapped = match self.kind.lock().call("client.fallback", serde_json::Value::Null) {
            Ok(v) => v.get("swapped").and_then(|b| b.as_bool()).unwrap_or(false),
            // A kind with no such method is not a failure, it is a kind with
            // no such method.
            Err(_) => return Ok(false),
        };
        if swapped {
            self.health.reset();
            self.start()?;
        }
        Ok(swapped)
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
    ///
    /// Only for a source declaring `restart-in-place`. The supervisor checks
    /// that before it gets here; a kind without it is built again from
    /// nothing.
    pub fn restart(&self) -> Result<()> {
        info!(source = %self.id, "restarting input pipeline");
        self.restart_armed.store(false, Ordering::SeqCst);
        self.pipeline.set_state(gst::State::Null).ok();
        for p in &self.placement {
            p.reset();
        }
        // Whatever the kind has to bring back with the pipeline: an exec
        // source's process is killed and a fresh fd handed to the same fdsrc,
        // so a command that exits is simply run again.
        self.kind.lock().call("restart", serde_json::Value::Null)?;
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
    ///
    /// Reported in the status extras. Nothing in the supervisor reads it any
    /// more: the two decisions it used to make are the two questions below,
    /// and both are answered from what the kind declared.
    pub fn superimposed(&self) -> bool {
        self.superimposed
    }

    /// Whether the supervisor may NULL this pipeline and start it again, or
    /// must build the source from nothing.
    pub fn restarts_in_place(&self) -> bool {
        self.capabilities.has(crate::plugin::Capability::RestartInPlace)
    }

    /// Whether this source puts its own output on the programme's timeline
    /// already, in which case the aligner must leave it alone. A source with
    /// its own compositor does, because that compositor runs on the clock and
    /// base time the mixer gave this pipeline.
    pub fn composites_its_own_timeline(&self) -> bool {
        self.capabilities.has(crate::plugin::Capability::Alpha)
    }

    /// Whether this source can be scrubbed at all, before asking the pipeline.
    /// A kind that never declares `seek` is not asked twice a second for the
    /// rest of the broadcast.
    pub fn declares_seek(&self) -> bool {
        self.capabilities.has(crate::plugin::Capability::Seek)
    }

    /// What each side of this source's layered compositor has done, for a
    /// layered source. See `LayerCounts`.
    pub fn layer_counts(&self) -> Option<&Arc<LayerCounts>> {
        self.counts.as_ref()
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
        if !self.declares_seek() {
            return false;
        }
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


/// Send a decoder's pads to the branches that want them, as they appear.
///
/// `flvdemux` names its pads; `uridecodebin` does not, so the media type on the
/// pad's caps is the fallback. The destinations are arguments rather than fixed
/// because a superimposed source runs two decoders in one bin, the page and its
/// video, and each one has its own branch to reach. A `None` destination means
/// that decoder's stream of that kind is deliberately dropped.
pub fn route_pads(
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
/// Both names are answered for: `gmx-browser-` is what this release's sidecar
/// makes, `lbx-browser-` is what a LiveboxMix sidecar an operator has not
/// replaced yet still makes. The old prefix goes away in the release after 0.2.
fn browser_profile_dirs(
    env: &std::collections::BTreeMap<String, String>,
    pid: u32,
) -> Vec<std::path::PathBuf> {
    let base = env
        .get("TMPDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    ["gmx-browser", "lbx-browser"].iter().map(|p| base.join(format!("{p}-{pid}"))).collect()
}

/// How long a child is given to stop on its own before it is killed.
///
/// The sidecar handles SIGTERM: it posts a quit to CEF's UI thread, the
/// message loop returns, the mux is finished, CEF is shut down and only then
/// does it remove its own profile directory (`browser/src/main.rs`). All of
/// that is seconds on a loaded box with nine helper processes to bring down.
/// The old wait was one second and it was measured against the wrong thing:
/// it polled `killpg(pgid, 0)`, which answers "is any member of this group
/// still alive", and a Chromium helper is a member, so the answer was always
/// yes and the SIGKILL always fired. The sidecar never reached its own
/// cleanup, and on air on 2026-09-12 that was one profile directory left
/// behind per browser started: a hundred of them in twenty-five minutes where
/// six sources were running. The wait is now on the direct child, which is
/// what says the sidecar finished, and it is long enough to let it.
///
/// It costs nothing on air because it does not happen on the mixer's thread.
/// See `bury_child`.
const CHILD_EXIT_GRACE: Duration = Duration::from_secs(8);

/// Take the profile directory of a sidecar that is already dead.
///
/// Retried, because "already dead" is not quite true the instant a SIGKILL is
/// sent: a helper that is still writing into the directory turns the removal
/// into a race that fails with ENOTEMPTY. A few tries over a second is enough
/// for the kernel to have finished with them.
fn remove_browser_profile(env: &std::collections::BTreeMap<String, String>, pid: u32) {
    for dir in browser_profile_dirs(env, pid) {
        remove_profile_dir(&dir);
    }
}

/// One profile directory, with the retries.
fn remove_profile_dir(dir: &std::path::Path) {
    let mut last = None;
    for i in 0..10 {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {
                debug!(path = %dir.display(), tries = i + 1, "removed the sidecar's profile directory");
                return;
            }
            // Not a browser source, the prefix this sidecar does not use, or
            // the sidecar got there first. Any of those is fine; anything else
            // is worth knowing about, because it is disk that will not come
            // back on its own.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => last = Some(e),
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    warn!(path = %dir.display(), ?last, "could not remove the sidecar's profile directory");
}

/// Every process started under `pid`, however deep, while the tree is still
/// standing.
///
/// Chromium's helpers are in the process group the mixer gave their parent, so
/// `killpg` reaches them; one that calls `setsid` is not, and after its parent
/// dies nothing connects it to this source any more. So they are written down
/// first, from the kernel's own view of the tree, and killed by pid afterwards
/// if they are still there. Linux only: `/proc/<pid>/task/<tid>/children` is
/// where this lives and there is no equivalent on macOS, where the sidecar is
/// a development convenience and the leak was never seen.
#[cfg(target_os = "linux")]
fn descendants(pid: u32) -> Vec<u32> {
    fn children_of(pid: u32, out: &mut Vec<u32>) {
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else { return };
        for task in tasks.flatten() {
            let Ok(text) = std::fs::read_to_string(task.path().join("children")) else { continue };
            for kid in text.split_ascii_whitespace().filter_map(|t| t.parse::<u32>().ok()) {
                if out.contains(&kid) {
                    continue;
                }
                out.push(kid);
                children_of(kid, out);
            }
        }
    }
    let mut out = Vec::new();
    children_of(pid, &mut out);
    out
}

#[cfg(not(target_os = "linux"))]
fn descendants(_pid: u32) -> Vec<u32> {
    Vec::new()
}

/// Kill a child, everything it started, and the profile directory it was too
/// dead to remove itself, on a thread of its own.
///
/// On a thread of its own because the caller is usually the mixer's own
/// thread, the one that answers every command, and the wait above is seconds.
/// It used to be done in line and the shorter wait it had was still long
/// enough to be felt on a rebuild.
///
/// `child` is moved in and waited on here, which is what keeps it from
/// becoming a zombie. Nothing else in this process waits for it.
fn bury_child(child: std::process::Child, env: std::collections::BTreeMap<String, String>) {
    let pid = child.id();
    // Handed over rather than moved, so that a machine too short of threads to
    // take it still gets the child killed, here, instead of leaking it.
    let work = Arc::new(Mutex::new(Some((child, env))));
    let mine = work.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("undertaker-{pid}"))
        .spawn(move || {
            if let Some((child, env)) = mine.lock().take() {
                take_down(child, env);
            }
        });
    if spawned.is_err() {
        if let Some((child, env)) = work.lock().take() {
            take_down(child, env);
        }
    }
}

/// Signal, wait, insist, reap, and sweep up. See `bury_child`.
fn take_down(mut child: std::process::Child, env: std::collections::BTreeMap<String, String>) {
    let pid = child.id();
    let started = Instant::now();
    // Written down before anything is signalled: after the parent dies a
    // process that called `setsid` cannot be traced back to it.
    let strays = descendants(pid);
    #[cfg(unix)]
    unsafe {
        // Politely first, so the sidecar runs its own shutdown and a capture
        // script's traps fire.
        libc::killpg(pid as i32, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    let _ = child.kill();

    // Wait for the direct child to exit, and on unix wait for it without
    // collecting it. `WNOWAIT` leaves the process a zombie, which keeps its
    // pid allocated, and its pid is the process group id of everything it
    // started. Reaping it first and then signalling that group is a signal
    // sent to whatever the kernel handed the number to next. Collected below,
    // once there is nothing left to signal.
    let mut clean = false;
    #[cfg(unix)]
    let mut signal_group = true;
    while started.elapsed() < CHILD_EXIT_GRACE {
        #[cfg(unix)]
        {
            let mut status = 0i32;
            let seen = unsafe {
                libc::waitpid(pid as i32, &mut status, libc::WNOHANG | libc::WNOWAIT)
            };
            if seen > 0 {
                clean = true;
                break;
            }
            if seen < 0 {
                // Somebody else collected it, so the pid is already free and
                // the group it named means nothing now.
                clean = true;
                signal_group = false;
                break;
            }
        }
        #[cfg(not(unix))]
        if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
            clean = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    #[cfg(unix)]
    if signal_group {
        unsafe {
            // Whether it went quietly or not. Either it is still running or it
            // is a zombie nobody has collected, and in both cases the pid, and
            // so the group, is still this tree's.
            libc::killpg(pid as i32, libc::SIGKILL);
            for stray in &strays {
                libc::kill(*stray as i32, libc::SIGKILL);
            }
        }
    }
    // And now collect it, so it is not a zombie for the life of the mixer.
    let _ = child.wait();
    remove_browser_profile(&env, pid);
    debug!(
        pid,
        clean,
        strays = strays.len(),
        took_ms = started.elapsed().as_millis() as u64,
        "stopped a child process and cleaned up after it"
    );
}

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
pub struct StderrReader {
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
pub struct ExecChild {
    /// None only between `drop` taking it and the struct going away.
    child: Option<std::process::Child>,
    /// The environment the child was started with, kept because it is what
    /// says where the sidecar put its profile directory. See
    /// `browser_profile_dirs`.
    env: std::collections::BTreeMap<String, String>,
    /// The read end of the child's stdout, held for as long as the element
    /// reads it. See `ExecStdout`.
    stdout: ExecStdoutHeld,
    stderr: Option<StderrReader>,
}

impl ExecChild {
    /// Take ownership of a freshly spawned process and its two pipes. Letting
    /// go of the result is what kills the process; see `Drop`.
    pub fn new(
        child: std::process::Child,
        env: std::collections::BTreeMap<String, String>,
        stdout: ExecStdoutHeld,
        stderr: Option<StderrReader>,
    ) -> Self {
        Self { child: Some(child), env, stdout, stderr }
    }
}

/// Letting go of one of these kills the process behind it.
///
/// It used to be the caller's job, through `InputPipeline::stop`, and every
/// path that dropped an `InputPipeline` without calling it leaked the whole
/// child: a zombie the mixer never waited on, its profile directory, the read
/// end of its stdout and the thread reading its stderr. There are about thirty
/// fallible steps in `build_kind` after the child is started and a dozen more
/// in `Mixer::add_source_with` before the source is stored, and any one of them
/// returning an error took that path. On air on 2026-09-12 the zombies and the
/// profile directories both grew at four a minute, which is one of each per
/// browser started, and that is the shape of a child nobody owns.
///
/// Now the child is owned by this struct and the struct is owned by the
/// pipeline, so the process cannot outlive it whatever goes wrong.
impl Drop for ExecChild {
    fn drop(&mut self) {
        // Our own ends of its pipes first, here, on this thread: the reader
        // thread joined and the descriptor `fdsrc` was reading closed. `fdsrc`
        // never closes a descriptor it did not open, so that one is ours to
        // let go of, and a grandchild holding the write end of stderr would
        // otherwise keep the reader thread alive for good. Two descriptors a
        // build, measured over thirty add and remove cycles on this machine.
        if let Some(r) = self.stderr.as_mut() {
            r.stop();
        }
        self.stdout.take();
        if let Some(child) = self.child.take() {
            bury_child(child, std::mem::take(&mut self.env));
        }
    }
}

/// Whatever has to be kept alive to keep the source element reading. On unix
/// that is the descriptor itself; on Windows a reader thread owns the pipe and
/// there is nothing left over to hold.
#[cfg(unix)]
pub type ExecStdoutHeld = Option<std::os::fd::OwnedFd>;
#[cfg(not(unix))]
pub type ExecStdoutHeld = Option<std::convert::Infallible>;

/// The descriptor stays owned. `into_raw_fd` gave it away, and `fdsrc` never
/// closes a descriptor it did not open itself (it only closes its own, from
/// the `fd://` URI handler), so every exec child ever started left the read
/// end of its stdout pipe open in this process: one of the two descriptors a
/// rebuild leaked on air on 2026-09-12. Held here instead, so that killing the
/// child closes it.
pub enum ExecStdout {
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
pub fn spawn_exec(id: &str, spec: &ExecSpec) -> Result<(ExecStdout, std::process::Child, Option<StderrReader>)> {
    let mut child = exec_process(spec, std::process::Stdio::piped())?;

    let stderr = child.stderr.take().map(|err| {
        let name = id.to_string();
        StderrReader::spawn(format!("exec-stderr-{id}"), err, move |line| {
            // Inside the instance's span, so `log.set {instance, level}`
            // reaches a sidecar's own output. See `observe::in_instance`.
            crate::observe::in_instance(&name, || debug!(source = %name, "{line}"));
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
pub fn attach_exec_stdout(_id: &str, src: &gst::Element, out: ExecStdout) -> ExecStdoutHeld {
    use std::os::fd::AsRawFd;
    let ExecStdout::Fd(fd) = out;
    src.set_property("fd", fd.as_raw_fd());
    Some(fd)
}

#[cfg(not(unix))]
pub fn attach_exec_stdout(id: &str, src: &gst::Element, out: ExecStdout) -> ExecStdoutHeld {
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

pub fn make_exec_source(id: &str, spec: &ExecSpec) -> Result<(gst::Element, ExecChild)> {
    let (out, child, stderr) = spawn_exec(id, spec)?;
    // Owned from here on, so that a failure below takes the process with it.
    let mut held = ExecChild { child: Some(child), env: spec.env.clone(), stdout: None, stderr };
    let src = new_exec_source(id)?;
    held.stdout = attach_exec_stdout(id, &src, out);
    Ok((src, held))
}

/// Build a headless browser rendering a page as a live source.
///
/// `wpesrc` runs a WPE WebKit instance offscreen and exposes what it draws and
/// plays as pads. It is packaged on Linux (`gstreamer1.0-wpe` on Debian and
/// Ubuntu, `gst-plugins-bad` with `wpewebkit` elsewhere) and is not available
/// on macOS, so say plainly what is missing rather than failing obscurely.
pub fn make_web_source(id: &str, uri: &str) -> Result<gst::Element> {
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
pub fn web_render_caps(canvas: &CanvasCaps) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("width", canvas.width)
        .field("height", canvas.height)
        .field("framerate", canvas.fps)
        .build()
}

/// Build an RTMP source element, tolerating the two implementations having
/// different property sets.
pub fn make_rtmp_source(element: &str, id: &str, uri: &str) -> Result<gst::Element> {
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

/// Where the last buffer to leave a source's normalising chain sat on the
/// programme's timeline.
///
/// Instrumentation for the stall of 2026-09-11 and 2026-09-12, where a
/// superimposed source came up with every layer placed normally and then never
/// delivered a picture, while its sound flowed. Both aggregators in that source
/// are force-live and emit black and silence on schedule with every layer dead,
/// so a source producing nothing means its output was blocked downstream, not
/// starved, and the only thing downstream that can block it is the programme
/// compositor holding buffers it is not ready to consume. That happens when
/// they are timed ahead of where the programme has got to, which is a number
/// nobody was writing down.
///
/// Two relaxed atomic stores per buffer and nothing else. The numbers are read
/// only when a source is judged stalled and once when its first picture
/// arrives; see `Mixer::timeline_of`.
#[derive(Debug, Default)]
pub struct LastBuffer {
    /// Running time of the last buffer through the probe, in nanoseconds.
    running_ns: AtomicU64,
    seen: AtomicU64,
}

impl LastBuffer {
    fn mark(&self, running: gst::ClockTime) {
        self.running_ns.store(running.nseconds(), Ordering::Relaxed);
        self.seen.fetch_add(1, Ordering::Relaxed);
    }

    /// Running time of the last buffer, or `None` if none has passed yet.
    pub fn running(&self) -> Option<gst::ClockTime> {
        (self.seen.load(Ordering::Relaxed) > 0)
            .then(|| gst::ClockTime::from_nseconds(self.running_ns.load(Ordering::Relaxed)))
    }

    pub fn seen(&self) -> u64 {
        self.seen.load(Ordering::Relaxed)
    }
}

/// Record where each buffer through this pad sits on the pipeline's timeline.
///
/// The segment is what turns a buffer's timestamp into a running time, and it
/// arrives as an event, so both are watched on the one probe. The segment is
/// cached under a lock that is taken once per buffer and is never contended:
/// one thread pushes this pad.
pub fn install_timeline_probe(element: &gst::Element, pad: &str, seen: &Arc<LastBuffer>) -> Result<()> {
    let pad = element
        .static_pad(pad)
        .with_context(|| format!("{} has no {pad} pad", element.name()))?;
    let segment: Mutex<Option<gst::FormattedSegment<gst::ClockTime>>> = Mutex::new(None);
    let seen = seen.clone();
    pad.add_probe(
        gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM,
        move |_pad, info| {
            match &info.data {
                Some(gst::PadProbeData::Event(e)) => {
                    if let gst::EventView::Segment(sg) = e.view() {
                        *segment.lock() = sg.segment().downcast_ref::<gst::ClockTime>().cloned();
                    }
                }
                Some(gst::PadProbeData::Buffer(b)) => {
                    if let (Some(pts), Some(sg)) = (b.pts(), segment.lock().as_ref()) {
                        if let Some(rt) = sg.to_running_time(pts) {
                            seen.mark(rt);
                        }
                    }
                }
                _ => {}
            }
            gst::PadProbeReturn::Ok
        },
    )
    .context("installing timeline probe")?;
    Ok(())
}

pub fn install_buffer_probe<F>(element: &gst::Element, pad: &str, f: F) -> Result<()>
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
pub fn optional_livesync(name: &str) -> Result<Option<gst::Element>> {
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
    use crate::state::SourceAudio;
    use super::*;
    use crate::config::{Accel, Canvas, RtmpClient, Superimpose};

    fn init() {
        let _ = gst::init();
    }

    fn test_source() -> SourceConfig {
        let mut cfg = SourceConfig::bare("cam1", "rtmp://127.0.0.1/live/cam1");
        cfg.name = Some("Camera 1".into());
        cfg.stall_timeout_secs = 2.0;
        cfg.rtmp_client = RtmpClient::Auto;
        cfg.superimpose = Superimpose::Off;
        cfg
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
        assert_eq!(
            input.thumb_proxy().expect("built with a thumbnail end").factory().unwrap().name(),
            "proxysink"
        );
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
                true,
                &browser,
                None,
                true,
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
        let dir = std::env::temp_dir().join(format!("gmx-sidecar-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("godwinmix-browser");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let canvas = CanvasCaps::new(&crate::config::Canvas::default());
        let mut browser = BrowserConfig {
            sidecar: Some(bin.to_string_lossy().to_string()),
            args: vec!["--verbose".into()],
            ..Default::default()
        };
        browser.env.insert("PULSE_SINK".into(), "gmx".into());

        let spec = ExecSpec::browser("web+https://example.com/x?a=1 b", &canvas, &browser)
            .unwrap()
            .expect("a configured sidecar is used");
        assert_eq!(spec.argv[0], bin.to_string_lossy());
        assert_eq!(&spec.argv[1..3], ["--url", "https://example.com/x?a=1 b"]);
        assert!(spec.argv.contains(&"--width".to_string()));
        assert_eq!(spec.argv.last().unwrap(), "--verbose");
        assert_eq!(spec.env.get("PULSE_SINK").unwrap(), "gmx");

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
                "godwinmix-browser".into(),
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
        let tmp = std::env::temp_dir().join(format!("gmx-profile-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let env: std::collections::BTreeMap<String, String> =
            [("TMPDIR".to_string(), tmp.to_string_lossy().to_string())].into_iter().collect();

        // Both prefixes, because a sidecar left over from LiveboxMix still
        // names its profile the old way. The new one comes first.
        let dirs = browser_profile_dirs(&env, 4242);
        assert_eq!(dirs, vec![tmp.join("gmx-browser-4242"), tmp.join("lbx-browser-4242")]);
        // A profile is a tree, not a file, and CEF leaves it locked open until
        // the process goes; nothing here may assume it is empty.
        for dir in &dirs {
            std::fs::create_dir_all(dir.join("Default/Cache")).unwrap();
            std::fs::write(dir.join("Default/Cache/data_0"), vec![0u8; 4096]).unwrap();
        }
        remove_browser_profile(&env, 4242);
        for dir in &dirs {
            assert!(!dir.exists(), "{} should be gone", dir.display());
        }

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
        // The background `sleep` is in the child's process group, which is
        // what takes it down.
        unsafe {
            libc::killpg(child.id() as i32, libc::SIGKILL);
        }
    }

    /// Every path that dropped an `InputPipeline` without calling `stop` used
    /// to leak the whole child: a process nobody waited on, and its profile
    /// directory. There are about thirty fallible steps in `build_kind` after
    /// the child is started and a dozen more in `Mixer::add_source_with`, and
    /// an error in any of them took that path. Dropping it has to be enough.
    #[test]
    #[cfg(unix)]
    fn an_abandoned_exec_child_takes_its_process_and_its_profile_with_it() {
        let _ = gst::init();
        let tmp = std::env::temp_dir().join(format!("gmx-drop-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let mut env = std::collections::BTreeMap::new();
        env.insert("TMPDIR".to_string(), tmp.to_string_lossy().to_string());
        let spec = ExecSpec { argv: shell_words::split("sh -c 'sleep 120'").unwrap(), env };

        let (_src, child) = make_exec_source("drop-test", &spec).unwrap();
        let pid = child.child.as_ref().expect("a freshly built child holds its process").id();
        // The directory the sidecar of this pid would have made for itself.
        let profile = browser_profile_dirs(&spec.env, pid).remove(0);
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("filler"), b"x").unwrap();

        drop(child);

        // The kill and the cleanup happen on a thread of their own, so that
        // removing a source does not block the mixer. Wait for them.
        let began = Instant::now();
        while began.elapsed() < CHILD_EXIT_GRACE + Duration::from_secs(5) {
            let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
            if !alive && !profile.exists() {
                let _ = std::fs::remove_dir_all(&tmp);
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
        let left = profile.exists();
        let _ = std::fs::remove_dir_all(&tmp);
        panic!("after dropping the child: process alive {alive}, profile left {left}");
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
