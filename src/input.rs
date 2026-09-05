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
use crate::config::{BrowserConfig, SourceConfig};
use crate::gstutil::{self, make};
use crate::probe::Backends;
use crate::state::{SourceHealth, SourceId, SourceState};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
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
    format!("file://{}", abs.display())
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
    /// Set when the pipeline posts an error; the supervisor restarts it.
    failed: Arc<AtomicBool>,
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
    exec_child: Mutex<Option<std::process::Child>>,
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
            dir.join(NAME),
            dir.join(format!("{NAME}.app")).join("Contents/MacOS").join(NAME),
        ];
        if let Some(p) = candidates.into_iter().find(|p| p.is_file()) {
            return Ok(Some(p));
        }
    }
    let found = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).map(|d| d.join(NAME)).find(|p| p.is_file()))
        .unwrap_or(None);
    Ok(found)
}

impl InputPipeline {
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
    ) -> Result<Self> {
        let id = cfg.id.clone();
        let mut exec_child: Option<std::process::Child> = None;
        // A page is rendered by the sidecar when there is one, and the sidecar
        // is just another process writing a container to stdout. From here on
        // such a source is an exec source in every respect.
        let exec = match kind {
            SourceKind::Exec => Some(ExecSpec::from_uri(&cfg.uri, allow_exec)?),
            SourceKind::Web => ExecSpec::browser(&cfg.uri, canvas, browser)?,
            _ => None,
        };
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

        // Entry points for dynamically added pads: the parsers for RTMP, the GL
        // conversion for a web page, the normaliser itself for an
        // already-decoded media URI.
        let video_entry = h264parse
            .clone()
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
        {
            let id = id.clone();
            let has_video = has_video.clone();
            let has_audio = has_audio.clone();
            dynamic.connect_pad_added(move |_el, pad| {
                // flvdemux names its pads; uridecodebin does not, so fall back
                // to the media type on the pad's caps.
                let name = pad.name();
                let media = pad
                    .current_caps()
                    .and_then(|c| c.structure(0).map(|s| s.name().to_string()))
                    .unwrap_or_default();

                let is_video = name.starts_with("video") || media.starts_with("video/");
                let is_audio = name.starts_with("audio") || media.starts_with("audio/");

                let target = if is_video {
                    has_video.store(true, Ordering::Relaxed);
                    &video_entry
                } else if is_audio {
                    has_audio.store(true, Ordering::Relaxed);
                    &audio_entry
                } else {
                    debug!(source = %id, pad = %name, %media, "ignoring unrecognised pad");
                    return;
                };

                let Some(sink) = target.static_pad("sink") else { return };
                if sink.is_linked() {
                    debug!(source = %id, pad = %name, "entry already linked, ignoring extra stream");
                    return;
                }
                match pad.link(&sink) {
                    Ok(_) => info!(source = %id, pad = %name, %media, "linked source pad"),
                    Err(e) => warn!(source = %id, pad = %name, ?e, "failed to link source pad"),
                }
            });
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
            failed: Arc::new(AtomicBool::new(false)),
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
        self.pipeline
            .set_state(gst::State::Playing)
            .with_context(|| format!("starting input pipeline for {}", self.id))?;
        Ok(())
    }

    pub fn stop(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        self.kill_exec_child();
    }

    fn kill_exec_child(&self) {
        let Some(mut child) = self.exec_child.lock().take() else { return };
        stop_process_group(child.id());
        let _ = child.wait();
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

        // An exec source's process has to come back with its pipeline. The old
        // one is killed and a fresh fd handed to the same fdsrc, so a command
        // that exits is simply run again.
        if let Some(spec) = &self.exec {
            self.kill_exec_child();
            match spawn_exec(&self.id, spec) {
                Ok((fd, child)) => {
                    self.source.lock().set_property("fd", fd);
                    *self.exec_child.lock() = Some(child);
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

/// Start the command behind an `exec:` source and hand its stdout to `fdsrc`.
///
/// Anything that can write a container to stdout becomes a source: ffmpeg, a
/// script, a purpose-built capture binary. Decoding happens downstream through
/// `decodebin`, which picks up the raised ranks of whatever hardware decoder
/// this machine has, so an exec source is accelerated on a GPU box and falls
/// back to software on one without, exactly like every other source.
fn spawn_exec(id: &str, spec: &ExecSpec) -> Result<(i32, std::process::Child)> {
    use std::os::fd::IntoRawFd;

    let (program, args) = spec.argv.split_first().context("exec source has an empty command")?;

    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .envs(&spec.env)
        .stdout(std::process::Stdio::piped())
        // Keep the child's noise out of our own stderr; it is logged separately.
        .stderr(std::process::Stdio::piped());

    // Put the child in its own process group so the whole tree can be signalled
    // together. A capture command is often a script that starts a browser and
    // an X server, and signalling only the script leaves those orphaned.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn().with_context(|| format!("starting `{program}`"))?;

    if let Some(err) = child.stderr.take() {
        let id = id.to_string();
        std::thread::Builder::new()
            .name(format!("exec-stderr-{id}"))
            .spawn(move || {
                use std::io::BufRead;
                for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
                    debug!(source = %id, "{line}");
                }
            })
            .ok();
    }

    let stdout = child.stdout.take().context("child produced no stdout")?;
    // Hand the descriptor to GStreamer. `into_raw_fd` gives up Rust's ownership
    // so the pipe is not closed when this goes out of scope.
    let fd = stdout.into_raw_fd();
    info!(source = %id, %program, "started exec source");
    Ok((fd, child))
}

fn make_exec_source(id: &str, spec: &ExecSpec) -> Result<(gst::Element, std::process::Child)> {
    let (fd, child) = spawn_exec(id, spec)?;
    let src = make("fdsrc", &format!("{id}-src-exec"))?;
    src.set_property("fd", fd);
    // Deliberately no do-timestamp. The process writes a container, and
    // stamping buffers with their arrival time before the demuxer sees them
    // destroys the timing the container carries. The demuxer's own timestamps
    // are the correct ones, and the mixer pad offset aligns them afterwards.
    Ok((src, child))
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
    use crate::config::{Accel, Canvas, RtmpClient};

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

    #[test]
    fn exec_sources_are_refused_unless_enabled() {
        let err = ExecSpec::from_uri("exec:echo hi", false).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("disabled"), "should say it is off: {msg}");
        assert!(msg.contains("allow_exec_sources"), "should name the setting: {msg}");

        // And an empty command is refused even when enabled.
        assert!(ExecSpec::from_uri("exec:", true).is_err());
    }

    #[test]
    fn an_enabled_exec_source_starts_its_process() {
        let spec = ExecSpec::from_uri("exec:sh -c 'printf hello; sleep 5'", true).unwrap();
        let (fd, mut child) = spawn_exec("s", &spec).unwrap();
        assert!(fd > 2, "should hand back a real pipe descriptor, got {fd}");
        // The descriptor is GStreamer's now; read it back to prove it is live.
        use std::io::Read;
        use std::os::fd::FromRawFd;
        let mut f = unsafe { std::fs::File::from_raw_fd(fd) };
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
}
