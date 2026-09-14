//! `layered/source`: a web page drawn over the video it was playing.
//!
//! The kind the audit called "a plugin sized feature the mixer special cases in
//! six places". It is one kind now. The page is rendered by the sidecar with
//! `--transparent --detect-media`, so Chromium neither paints nor decodes the
//! video it was showing; the mixer decodes that video itself, places it on a
//! compositor of this source's own, and draws the page over the top. What
//! leaves this pipeline is exactly what leaves every other source, so nothing
//! downstream knows the difference.
//!
//! It is also the one kind that does not declare `restart-in-place`: brought
//! back in place the layered pipeline's audio mixer spun on a failing latency
//! query and its compositor managed under a frame a second. It is built again
//! from nothing instead, which the supervisor now decides from the capability
//! rather than from a flag.

use super::exec::ExecProcess;
use super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::{self, make};
use crate::input::{file_uri, ExecSpec, MediaItem, MediaReport};
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds,
    PluginState, ProvideKind, Ready, StreamMode, Tier, API_LEVEL,
};
use crate::state::{SourceAudio, SourceId};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

pub const MANIFEST: Manifest = Manifest {
    plugin: "layered",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description:
        "A web page drawn over the video it was playing, decoded here rather than in the browser",
    // Never claimed from a bare URI: the core substitutes this kind for
    // `browser/source` once a page has been probed and has media worth taking
    // over. An operator can still write `type = "layered/source"`.
    uri_schemes: &[],
    rank: 0,
    media: MediaDecl {
        video: StreamMode::Container,
        audio: StreamMode::Container,
        alpha: true,
        thumb: true,
    },
    capabilities: CapabilitySet::new()
        .with(Capability::Health)
        .with(Capability::AudioLayers)
        .with(Capability::Alpha),
    latency_ms: 500,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide {
    manifest: MANIFEST,
    claims,
    make: new,
};

fn claims(_uri: &str) -> Option<u16> {
    None
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    let report = req
        .overlay
        .clone()
        .context("a layered source needs the page's media report; probe the page first")?;
    let mut spec = ExecSpec::browser(req.cfg.uri.as_str(), req.canvas, req.browser)?
        .context("a layered source needs the browser sidecar; none is configured")?;
    // `--transparent` gives the page a real alpha channel, and with
    // `--detect-media` it also hides and pauses the video the mixer is taking
    // over, so Chromium neither paints nor decodes it. It costs the page's
    // audio, which is why the media's audio is used instead.
    spec.argv.push("--transparent".into());
    spec.argv.push("--detect-media".into());
    // And it draws slower, because now it is only drawing chrome. An alpha
    // frame is 4 bytes a pixel where I420 is 1.5, so a 720p page at the canvas
    // rate would put 107 MB/s down a pipe that carries 41 MB/s for an ordinary
    // source.
    spec.set_fps(req.browser.overlay_fps);
    Ok(Box::new(LayeredSource {
        ctx: req.ctx(),
        process: ExecProcess::new(spec),
        report,
        cache: Vec::new(),
        levels: None,
        running: false,
    }))
}

pub struct LayeredSource {
    ctx: BuildCtx,
    process: ExecProcess,
    report: MediaReport,
    /// Local copies of the page's videos, removed when the source stops.
    cache: Vec<std::path::PathBuf>,
    levels: Option<AudioLevels>,
    running: bool,
}

impl Source for LayeredSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        validate(&hello.params)?;
        self.ctx.canvas = hello.canvas;
        Ok(Ready {
            manifest: MANIFEST,
            latency_ms: MANIFEST.latency_ms,
            capabilities: MANIFEST.capabilities,
        })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let id = self.ctx.id.clone();
        let layers = Layers::build(&id, &self.report)?;
        let src = self.process.build_src(&id)?;
        let decode = ExecProcess::decoder(&id)?;
        self.cache = self
            .report
            .media
            .iter()
            .filter_map(|m| m.cache.clone())
            .collect();
        let levels = layers.levels();
        self.levels = Some(levels.clone());

        let mut els = vec![src.clone(), decode.clone()];
        els.extend(layers.elements().into_iter().cloned());
        let canvas = canvas.clone();
        let ends = assemble(
            &self.ctx,
            thumb,
            Ingest::default().with(els).livesync(false),
            move |w: &Wiring| {
                gst::Element::link(&src, &decode).context("linking the sidecar to the decoder")?;
                // On the layered path this decoder is the page, and whatever
                // the page still plays once its videos are taken over goes into
                // the mix with them; see `Layers::link`.
                w.route(&decode, layers.over_q.clone(), layers.page_aconv.clone());
                layers
                    .link(&canvas, &w.norm.video_entry(), &w.norm.audio_entry())
                    .context("linking the superimposed layers")?;
                // Before the routing below, so the placement probes are on each
                // new pad before anything is linked to it.
                let placement = layers.place_in_time(&w.ctx.id, &decode)?;
                // The page's videos, decoded here, each into its own branch.
                for b in &layers.media {
                    w.route(&b.src, b.conv.clone(), b.aq.clone());
                }
                Ok(KindParts {
                    superimposed: true,
                    layer_counts: Some(layers.counts.clone()),
                    levels: Some(levels.clone()),
                    placement,
                    ..KindParts::default()
                })
            },
        )?;
        self.running = true;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.process.kill(&self.ctx.id);
        for f in &self.cache {
            let _ = std::fs::remove_file(f);
        }
        self.cache.clear();
        self.running = false;
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        validate(params)?;
        Ok(Configure::RestartRequired(
            "a layered page is probed again before it can take a new address".into(),
        ))
    }

    fn health(&self) -> Health {
        Health::of(if self.running {
            PluginState::Running
        } else {
            PluginState::Starting
        })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            // Built again from nothing rather than in place: see the module
            // comment. The supervisor knows this from the capability and does
            // not call `restart` here, so this says so rather than pretending.
            "restart" => anyhow::bail!(
                "layered/source does not restart in place; the core builds it again from nothing"
            ),
            "audio.set" => {
                let levels = self
                    .levels
                    .as_ref()
                    .context("the layers are not built yet")?;
                let page = params.get("page").and_then(|v| v.as_f64());
                let media: Vec<Option<f64>> = params
                    .get("media")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().map(|v| v.as_f64()).collect())
                    .unwrap_or_default();
                let after = levels.apply(page, &media);
                Ok(serde_json::to_value(after)?)
            }
            "audio.get" => {
                let levels = self
                    .levels
                    .as_ref()
                    .context("the layers are not built yet")?;
                Ok(serde_json::to_value(levels.report())?)
            }
            "sidecar" => Ok(json!({ "sidecar": true })),
            other => Err(unknown_method(
                &MANIFEST,
                other,
                &["audio.set", "audio.get", "sidecar"],
            )),
        }
    }
}

pub fn validate(params: &Params) -> Result<()> {
    for (key, value) in params {
        match key.as_str() {
            "uri" | "url" => {
                anyhow::ensure!(
                    value.is_str(),
                    "layered/source params.{key} must be a string"
                );
            }
            "superimpose" => {
                let s = value.as_str().unwrap_or_default();
                anyhow::ensure!(
                    matches!(s, "off" | "auto"),
                    "layered/source params.superimpose must be off or auto, not `{s}`"
                );
            }
            _ => {}
        }
    }
    Ok(())
}

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
pub enum Fetched {
    /// A stream: nothing to fetch, play it from the address.
    Stream,
    /// A clip, now on disk, and `src` points at the copy.
    Copy(std::path::PathBuf),
    /// Neither: the address could not be read at all.
    Failed,
}

pub fn cache_media(id: &SourceId, src: &mut String) -> Fetched {
    let path_part = src
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
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
    let stem = path_part
        .rsplit('/')
        .next()
        .unwrap_or("clip")
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '.', "_");
    // Numbered per fetch, not only per process: a source rebuilt after its
    // browser died fetches its clips again while the old pipeline, torn down
    // on another thread, is deleting its own, and with the same names the new
    // copy would go with the old.
    static FETCHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = FETCHES.fetch_add(1, Ordering::SeqCst);
    let file =
        std::env::temp_dir().join(format!("gmx-media-{id}-{}-{n}-{stem}", std::process::id()));
    let fetch = || -> Result<()> {
        let pipeline = gst::Pipeline::with_name(&format!("fetch-{id}"));
        let http = make(factory, &format!("{id}-fetch-src"))?;
        http.set_property("location", &*src);
        let sink = make("filesink", &format!("{id}-fetch-sink"))?;
        sink.set_property("location", file.to_string_lossy().as_ref());
        pipeline
            .add_many([&http, &sink])
            .context("adding fetch elements")?;
        http.link(&sink).context("linking fetch")?;
        pipeline
            .set_state(gst::State::Playing)
            .context("starting fetch")?;
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
    counts: Arc<LayerCounts>,
}

/// What each side of a layered source's compositor has done, counted where it
/// happens.
///
/// Instrumentation for the stall of 2026-09-11 and 2026-09-12. What the
/// mixer could see from its own side was that a superimposed source delivered
/// four to seven frames and then no more while its sound went on flowing, and
/// that both of the programme's queues for it were empty, so nothing
/// downstream was holding it. That leaves four places it can have stopped and
/// no way to tell them apart from outside: the page's browser, the videos the
/// mixer decodes, the compositor that blends them, or the videorate behind it.
/// One counter on each pad says which, and costs one relaxed add per buffer.
#[derive(Debug, Default)]
pub struct LayerCounts {
    /// Frames arriving at the compositor from the page's browser.
    pub page_in: AtomicU64,
    /// Frames arriving at the compositor from the videos the mixer decodes,
    /// all layers together.
    pub media_in: AtomicU64,
    /// Frames the compositor produced. A force-live aggregator keeps
    /// producing with every input dead, so this standing still while the two
    /// above climb is the compositor's own fault and nobody else's.
    pub comp_out: AtomicU64,
    /// And what came out of the videorate that follows it. Less than
    /// `comp_out` means the frames existed and were dropped for their
    /// timestamps, which is a different fault with the same symptom.
    pub rate_out: AtomicU64,
    /// Buffers the layered audio mixer produced. The sibling that kept
    /// running through the stall, kept here so the two can be read together.
    pub mix_out: AtomicU64,
}

/// Count every buffer leaving `element` into one of the fields above.
///
/// The closure holds the counters and nothing else; the pad it sits on is
/// reached through the probe's own argument. See `Placement::watch` for why
/// that matters.
fn count_buffers(
    element: &gst::Element,
    counts: &Arc<LayerCounts>,
    field: fn(&LayerCounts) -> &AtomicU64,
) -> Result<()> {
    let pad = element
        .static_pad("src")
        .with_context(|| format!("{} has no src pad to count", element.name()))?;
    let counts = counts.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
        field(&counts).fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    })
    .context("installing a layer counter")?;
    Ok(())
}

impl LayerCounts {
    pub fn read(&self) -> [u64; 5] {
        [
            self.page_in.load(Ordering::Relaxed),
            self.media_in.load(Ordering::Relaxed),
            self.comp_out.load(Ordering::Relaxed),
            self.rate_out.load(Ordering::Relaxed),
            self.mix_out.load(Ordering::Relaxed),
        ]
    }
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
                &gst::Caps::builder("video/x-raw")
                    .field("format", "I420")
                    .build(),
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
            &self.src,
            &self.conv,
            &self.caps,
            &self.q,
            &self.scale,
            &self.aq,
            &self.aconv,
            &self.ares,
            &self.avol,
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
        anyhow::ensure!(
            !media.is_empty(),
            "a layered source needs at least one video"
        );
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
                &gst::Caps::builder("video/x-raw")
                    .field("format", "AYUV")
                    .build(),
            )?,
            // And back to I420 immediately, so what leaves this bin is what
            // every other source produces and nothing downstream has to know
            // the page ever had transparency.
            flat_conv: make("videoconvert", &format!("{id}-sup-flat-conv"))?,
            flat_caps: gstutil::capsfilter(
                &format!("{id}-sup-flat-caps"),
                &gst::Caps::builder("video/x-raw")
                    .field("format", "I420")
                    .build(),
            )?,
            counts: Arc::new(LayerCounts::default()),
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
        //
        // The size is pinned as well, and that is newer. Every pad on this
        // compositor is placed inside the canvas and the page's pad covers it
        // exactly, so the composite was always canvas-sized; saying so leaves
        // the compositor nothing to work out. It is also the caps the query
        // below answers with, and an answer has to be one thing.
        let composed = gst::Caps::builder("video/x-raw")
            .field("format", "AYUV")
            .field("width", canvas.width)
            .field("height", canvas.height)
            .field("framerate", canvas.fps)
            .build();
        self.comp_caps.set_property("caps", &composed);
        // And the compositor is told that answer directly, so that working out
        // its output caps cannot reach the programme pipeline. See
        // `answer_negotiation_here`: this is where a source stopped delivering
        // video while its sound went on.
        gstutil::answer_negotiation_here(&self.comp, &composed)?;

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
            let (w, h) = (
                (w + 2).min(canvas.width - x),
                (h + 2).min(canvas.height - y),
            );
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
            info!(
                video = b.item.index,
                x,
                y,
                width = w,
                height = h,
                muted = b.item.muted,
                "page video placed on the canvas"
            );
            media_pads.push(pad);
        }

        // The page arrives from the sidecar already keyed: alpha zero where a
        // video the mixer draws itself used to be. See `KEY_TOLERANCE` in the
        // sidecar's mux.rs for why that happens there and not here.
        gst::Element::link_many([&self.over_q, &self.over_conv])
            .context("linking the page branch")?;
        gst::Element::link_many([
            &self.page_aconv,
            &self.page_ares,
            &self.page_vol,
            &self.amix,
        ])
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

        gst::Element::link_many([
            &self.comp,
            &self.comp_caps,
            &self.flat_conv,
            &self.flat_caps,
            vrate,
        ])
        .context("linking the composed layers into the normaliser")?;
        self.amix
            .link(audio_entry)
            .context("linking the mix into the audio chain")?;

        // One counter on each side of the compositor. See `LayerCounts`.
        for b in &self.media {
            count_buffers(&b.scale, &self.counts, |c| &c.media_in)?;
        }
        count_buffers(&self.over_conv, &self.counts, |c| &c.page_in)?;
        count_buffers(&self.comp, &self.counts, |c| &c.comp_out)?;
        count_buffers(vrate, &self.counts, |c| &c.rate_out)?;
        count_buffers(&self.amix, &self.counts, |c| &c.mix_out)?;
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
            if name.starts_with("video")
                || media.starts_with("video/")
                || name.starts_with("audio")
                || media.starts_with("audio/")
            {
                me.watch(&pid, Stream::Page, pad, None);
            }
        });
        all.push(page);

        for b in &self.media {
            let placement = Placement::new();
            // Only a clip held locally is looped. A stream is played from its
            // address and left to end; see `cache_media`.
            let loops = b.item.cache.is_some();
            // The decoder's pads only exist once it has connected, so they are
            // watched as they appear. The same caps test `route_pads` makes.
            let (me, id) = (placement.clone(), id.clone());
            b.src.connect_pad_added(move |el, pad| {
                // By name as well as by caps: a decoder can announce a pad
                // before its caps are known, and one classified by caps alone
                // was skipped here, never placed, and every frame of it late.
                let name = pad.name();
                let media = pad
                    .current_caps()
                    .and_then(|c| c.structure(0).map(|s| s.name().to_string()))
                    .unwrap_or_default();
                // Weak, and taken from the callback's own argument rather than
                // from a clone captured here. A strong clone of this decoder,
                // held in a closure this decoder owns, is a reference cycle
                // that GObject does not collect: measured with the leaks
                // tracer over ten build and remove cycles, one GstURIDecodeBin
                // and about fifteen of its pads stayed alive per cycle, for
                // ever. See `Placement::watch` for the other half of it.
                let again = loops.then(|| el.downgrade());
                if name.starts_with("video") || media.starts_with("video/") {
                    me.watch(&id, Stream::Video, pad, again.clone());
                } else if name.starts_with("audio") || media.starts_with("audio/") {
                    me.watch(&id, Stream::Audio, pad, again);
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
pub enum Stream {
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
pub struct Placement {
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
    pub fn new() -> Arc<Self> {
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
            .unwrap_or_else(|| {
                gst::ClockTime::from_nseconds(self.started.lock().elapsed().as_nanos() as u64)
            })
    }

    pub fn reset(&self) {
        *self.end.lock() = gst::ClockTime::ZERO;
        *self.prev_end.lock() = gst::ClockTime::ZERO;
        self.video_rounds.store(0, Ordering::SeqCst);
        self.audio_rounds.store(0, Ordering::SeqCst);
        self.ended.store(0, Ordering::SeqCst);
        self.restarting.store(false, Ordering::SeqCst);
        *self.started.lock() = Instant::now();
    }
    /// Watch the segments and buffers passing `probe_on` and keep that same
    /// pad placed.
    ///
    /// Nothing in the probe below holds a strong reference to a GStreamer
    /// object, and that is deliberate. The probe belongs to the pad, so a
    /// captured clone of the pad, or of the element that owns it, is a cycle
    /// with no collector: the earlier version captured both, and the leaks
    /// tracer counted one GstURIDecodeBin, two GstDecodePads and about
    /// thirteen ghost and proxy pads still alive per build after ten build and
    /// remove cycles. The pad comes from the probe's own argument and the
    /// decoder from a weak reference.
    pub fn watch(
        self: &Arc<Self>,
        id: &SourceId,
        stream: Stream,
        probe_on: &gst::Pad,
        again: Option<glib::WeakRef<gst::Element>>,
    ) {
        // The offset goes on this very pad, a source pad. Set on a sink pad
        // downstream, an offset takes effect only if the segment has not got
        // there yet, and where nothing sits between the two it always had: the
        // page's frames and the media's sound were each placed and then
        // dropped as old, every one. A source pad resends its segment with the
        // new offset before its next buffer, whenever the offset changes.
        self.streams.fetch_or(stream_bit(stream), Ordering::SeqCst);
        let watch = Watch {
            me: self.clone(),
            id: id.clone(),
            stream,
            segment: Mutex::new(None),
            pending: AtomicBool::new(false),
            placed_at: Mutex::new(gst::ClockTime::ZERO),
            placed_after: Mutex::new(gst::ClockTime::ZERO),
            own_change: AtomicBool::new(false),
            frames: AtomicU64::new(0),
            again,
        };
        probe_on.add_probe(
            gst::PadProbeType::EVENT_DOWNSTREAM | gst::PadProbeType::BUFFER,
            move |on, info| match &info.data {
                Some(gst::PadProbeData::Event(e)) => match e.view() {
                    gst::EventView::Segment(sg) => {
                        let seg = sg.segment().downcast_ref::<gst::ClockTime>().cloned();
                        watch.on_segment(on, seg);
                        gst::PadProbeReturn::Ok
                    }
                    gst::EventView::Eos(_) => watch.on_eos(on),
                    _ => gst::PadProbeReturn::Ok,
                },
                Some(gst::PadProbeData::Buffer(b)) => {
                    watch.on_buffer(on, b);
                    gst::PadProbeReturn::Ok
                }
                _ => gst::PadProbeReturn::Ok,
            },
        );
    }
}

/// What one placement probe carries between the events and buffers it sees.
///
/// This was all captured by a single 283 line closure. Splitting it into a
/// struct with three methods changes nothing about what happens, and makes
/// each part readable on its own.
struct Watch {
    me: Arc<Placement>,
    id: SourceId,
    stream: Stream,
    /// This pad's segment, kept to turn buffer times into running time, with
    /// the offset the pad had already folded into it. A pad applies its offset
    /// to a segment before any probe sees it (gstpad.c,
    /// gst_pad_push_event_unchecked), so the segment stored here is the
    /// shifted one, and a running time read from it includes the last
    /// placement. Taking that placement back out gives the stream's own time,
    /// which is what a new placement must be computed from: computed from the
    /// shifted time instead, each placement undid the one before it, and half
    /// the page's frames landed at the start of time.
    segment: Mutex<Option<(gst::FormattedSegment<gst::ClockTime>, gst::ClockTime)>>,
    /// Placed at the segment, which is the moment the offset can still take
    /// effect on the pad downstream, and refined on the first buffer if its
    /// timestamp within the segment is not zero, which a demuxer's segment
    /// normally makes it. That refinement only works where a queue sits
    /// between here and the pad, so the layers are probed upstream of one.
    pending: AtomicBool,
    placed_at: Mutex<gst::ClockTime>,
    placed_after: Mutex<gst::ClockTime>,
    /// Changing this pad's offset makes it resend its segment, and that resend
    /// comes straight back through this probe. Left alone it read as a new
    /// segment, was placed again, changed the offset again, and so on for
    /// every buffer: a dozen corrections a second and no layer ever settled.
    /// Each change of ours is marked and its one resend ignored.
    own_change: AtomicBool,
    /// Frames seen, for the page's occasional account of itself in the log.
    frames: AtomicU64,
    /// The decoder to start again at the end of a round, weakly. See `watch`.
    again: Option<glib::WeakRef<gst::Element>>,
}

impl Watch {
    /// A new segment: place this layer where the round before it ended, or at
    /// `now` with the stream's own bias when there is nothing to join.
    fn on_segment(&self, on: &gst::Pad, seg: Option<gst::FormattedSegment<gst::ClockTime>>) {
        let folded = gst::ClockTime::from_nseconds(on.offset().max(0) as u64);
        *self.segment.lock() = seg.map(|sg| (sg, folded));
        if self.own_change.swap(false, Ordering::SeqCst) {
            // The resend our own offset change caused.
            return;
        }
        let me = &self.me;
        let now = me.now(on);
        let after = match self.stream {
            Stream::Page => gst::ClockTime::ZERO,
            Stream::Video => {
                let end = *me.end.lock();
                *me.prev_end.lock() = end;
                me.video_rounds.fetch_add(1, Ordering::SeqCst);
                end
            }
            Stream::Audio => {
                // The picture's end. If the picture's own new round has
                // already begun it is frozen in `prev_end`; if not, the old
                // picture has fully drained by the time the new sound arrives,
                // so `end` is final.
                let k = me.audio_rounds.fetch_add(1, Ordering::SeqCst) + 1;
                if me.video_rounds.load(Ordering::SeqCst) >= k {
                    *me.prev_end.lock()
                } else {
                    *me.end.lock()
                }
            }
        };
        let place = biased(self.stream, now, after);
        place_offset(on, &self.own_change, place);
        *self.placed_at.lock() = place;
        *self.placed_after.lock() = after;
        self.pending.store(true, Ordering::SeqCst);
        if after == gst::ClockTime::ZERO {
            info!(
                source = %self.id,
                layer = ?self.stream,
                at_ms = place.mseconds(),
                "layer placed on the composite's timeline"
            );
        } else {
            info!(
                source = %self.id,
                layer = ?self.stream,
                at_ms = place.mseconds(),
                // What the viewer sees: how far past the end of the round
                // before this one starts, not how late its first buffer was.
                gap_ms = place.saturating_sub(after).mseconds(),
                late_ms = now.saturating_sub(after).mseconds(),
                "placed the next round of the page's media"
            );
        }
    }

    /// The clip has run out. For one held locally the decoder is started again
    /// from the copy, and the end of stream is kept from the compositor, which
    /// would otherwise mark the layer finished and ignore everything after. A
    /// stream is left to end.
    fn on_eos(&self, on: &gst::Pad) -> gst::PadProbeReturn {
        // A decoder that has been taken down with its source gives None here,
        // and then there is nothing to start again anyway.
        let Some(el) = self.again.as_ref().and_then(|w| w.upgrade()) else {
            if self.stream == Stream::Page {
                // The browser drawing the page has gone: it crashed, or its
                // container was stopped. Left alone, the compositor marks this
                // one pad finished and goes on compositing the videos over
                // black, and the source reports itself live for as long as they
                // play. That is the one failure a viewer sees and nothing
                // reports. Posted as an error on the bus, it is the
                // supervisor's ordinary restart: a fresh browser, the same
                // source.
                if let Some(parent) = on.parent_element() {
                    warn!(source = %self.id, "the page's browser stopped; restarting the source");
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
        let me = &self.me;
        me.ended.fetch_or(stream_bit(self.stream), Ordering::SeqCst);
        if self.stream == Stream::Video && !me.restarting.swap(true, Ordering::SeqCst) {
            let (el, me, id) = (el.clone(), me.clone(), self.id.clone());
            // Off the streaming thread. A state change made from a probe on the
            // element's own pad deadlocks against the thread it is asking to
            // stop.
            std::thread::spawn(move || restart_round(el, me, id));
        }
        gst::PadProbeReturn::Drop
    }

    /// A buffer: the page is restamped to now, a media layer's first buffer
    /// refines the placement its segment chose, and a video's end of round is
    /// written down as it passes.
    fn on_buffer(&self, on: &gst::Pad, b: &gst::Buffer) {
        let Some(pts) = b.pts() else { return };
        let guard = self.segment.lock();
        let Some((seg, folded)) = guard.as_ref() else {
            return;
        };
        // The stream's own running time, placement taken out.
        let rt = seg
            .to_running_time(pts)
            .unwrap_or(gst::ClockTime::ZERO)
            .saturating_sub(*folded);
        drop(guard);
        let now = self.me.now(on);
        if self.stream == Stream::Page {
            self.restamp_page(on, rt, now);
        } else if self.pending.swap(false, Ordering::SeqCst) {
            self.refine_placement(on, rt, now);
        }
        if self.stream == Stream::Video {
            // Running time at this pad, where the offset is applied, not here.
            let dur = b.duration().unwrap_or(gst::ClockTime::ZERO);
            let at = rt + dur + gst::ClockTime::from_nseconds(on.offset().max(0) as u64);
            let mut end = self.me.end.lock();
            if at > *end {
                *end = at;
            }
        }
    }

    /// The page is chrome, and the right time to show a frame of it is the
    /// moment it arrives. So every frame is stamped to now as it passes. The
    /// branch behind it can run late by whatever the pipe and the machine make
    /// it, and nothing is ever old: the earlier way, placing the page once, had
    /// the compositor skipping 132 of 134 frames as late.
    fn restamp_page(&self, on: &gst::Pad, rt: gst::ClockTime, now: gst::ClockTime) {
        self.pending.store(false, Ordering::SeqCst);
        let want = now.saturating_sub(rt);
        let have = gst::ClockTime::from_nseconds(on.offset().max(0) as u64);
        let drift = want.max(have) - want.min(have);
        // Only when the frame would otherwise land outside a small window
        // around now. Every change of offset resends the segment, and a page
        // that stamped each frame sent the compositor two events per frame.
        let restamped = drift > gst::ClockTime::from_nseconds(PAGE_DRIFT_NS);
        if restamped {
            place_offset(on, &self.own_change, want);
        }
        let n = self.frames.fetch_add(1, Ordering::SeqCst);
        if n == 0 || n % 50 == 0 {
            info!(
                source = %self.id,
                frame = n,
                own_ms = rt.mseconds(),
                at_ms = (rt + gst::ClockTime::from_nseconds(on.offset().max(0) as u64)).mseconds(),
                now_ms = now.mseconds(),
                restamped,
                "page frame through the placement probe"
            );
        }
    }

    /// The segment said where the layer starts; the first buffer says when it
    /// really arrived and what its own clock read. Sound turns up a third of a
    /// second after its segment while its decoder starts, and a sample that
    /// reaches the mix behind its output position is dropped. Place the buffer,
    /// not the segment: this pad resends the segment with the corrected offset
    /// before the buffer after this one.
    fn refine_placement(&self, on: &gst::Pad, rt: gst::ClockTime, now: gst::ClockTime) {
        let placed = *self.placed_at.lock();
        let after = *self.placed_after.lock();
        // A round that has one before it keeps the join the segment chose. The
        // sound's first buffer can be half a second behind its own segment
        // while the decoder starts, and biasing it to now all over again opened
        // exactly the gap the join is there to close: the picture carried on
        // and the sound came back three quarters of a second later. Only a
        // first round, which has nothing to join, is placed by its first
        // buffer.
        let place = if after.is_zero() {
            biased(self.stream, now, after)
        } else {
            placed
        };
        place_offset(on, &self.own_change, place.saturating_sub(rt));
        if now > placed + gst::ClockTime::from_mseconds(100)
            || rt > gst::ClockTime::from_mseconds(20)
        {
            info!(
                source = %self.id,
                layer = ?self.stream,
                at_ms = place.mseconds(),
                first_ms = rt.mseconds(),
                arrived_late_ms = now.saturating_sub(placed).mseconds(),
                "placement corrected by the first buffer"
            );
        }
    }
}

/// Start the page's media again from its local copy, off the streaming thread.
///
/// The picture and the sound each have a queue of the same length, so they
/// reach the end of the round within a moment of each other; wait for the sound
/// before pulling the decoder down, or the last of it never gets pushed.
/// Bounded, so a stream that never ends cannot stop the loop for good.
fn restart_round(el: gst::Element, me: Arc<Placement>, id: SourceId) {
    let started = Instant::now();
    let want = me.streams.load(Ordering::SeqCst);
    while me.ended.load(Ordering::SeqCst) != want && started.elapsed() < MEDIA_END_WAIT {
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
    pub page: gst::Element,
    pub media: Vec<gst::Element>,
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
        self.media
            .iter()
            .map(|v| v.property::<f64>("volume"))
            .collect()
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
        SourceAudio {
            page: self.page_gain(),
            media: self.media_gains(),
        }
    }
}
