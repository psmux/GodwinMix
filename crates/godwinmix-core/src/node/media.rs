//! Media between a node and the core.
//!
//! Raw video does not cross a network. 1080p30 I420 is 93 MB/s and the audit
//! measured a sidecar pipe already carrying 41 MB/s at 720p, so between hosts
//! the node encodes once with its own catalogue entry and the core decodes
//! once on its usual hardware aware path. That is the same price a browser
//! source pays today, and it buys the machine boundary.
//!
//! Three ways across, chosen by `transport` in the source's config:
//!
//! | transport | how | for |
//! |---|---|---|
//! | `rtp` | RTP over UDP, `rtpbin` with `ntp-time-source=clock-time` | a LAN. The receiver knows the sender's timeline from the first packet |
//! | `srt` | one MPEG-TS over SRT, ARQ, latency negotiated | a link that loses packets |
//! | `whip` | WebRTC | a WAN, or a NAT nobody controls |
//!
//! Whichever it is, one latency budget is declared at ingress and answered on
//! the LATENCY query, which is what keeps two remote cameras in lip sync.

use super::wire::{BridgeTransport, MediaPlan};
use crate::caps::CanvasCaps;
use crate::gstutil::make;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

/// The first port the core hands out for node media. Well above anything a
/// media server claims, and a contiguous block so one firewall rule covers it.
pub const FIRST_MEDIA_PORT: u16 = 8500;

/// How many ports the pool holds. Two per RTP source (video and audio) plus
/// headroom for sources coming and going.
pub const MEDIA_PORTS: u16 = 200;

/// Hands out media ports, and takes them back when a source goes.
#[derive(Default)]
pub struct PortPool {
    taken: parking_lot::Mutex<std::collections::BTreeSet<u16>>,
}

impl PortPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// `count` consecutive free ports, or an error naming the range.
    pub fn take(&self, count: u16) -> Result<u16> {
        let mut taken = self.taken.lock();
        let mut base = FIRST_MEDIA_PORT;
        while base + count <= FIRST_MEDIA_PORT + MEDIA_PORTS {
            if (0..count).all(|i| !taken.contains(&(base + i)) && free(base + i)) {
                for i in 0..count {
                    taken.insert(base + i);
                }
                return Ok(base);
            }
            base += 1;
        }
        anyhow::bail!(
            "no free media port between {FIRST_MEDIA_PORT} and {}. Remove a remote source, or \
             widen the range",
            FIRST_MEDIA_PORT + MEDIA_PORTS
        )
    }

    pub fn give_back(&self, base: u16, count: u16) {
        let mut taken = self.taken.lock();
        for i in 0..count {
            taken.remove(&(base + i));
        }
    }
}

/// Whether a UDP port can be bound right now. Cheap, and it catches the case
/// the pool cannot see: something outside GodwinMix already has it.
fn free(port: u16) -> bool {
    std::net::UdpSocket::bind(("0.0.0.0", port)).is_ok()
}

/// Settle how one remote source's media will travel.
///
/// `media_host` is where the node is, which the core learns from the socket
/// the node arrived on. `core_host` is where the node should send, which the
/// node learns from the address it dialled.
pub fn plan(
    transport: BridgeTransport,
    media_host: &str,
    core_host: &str,
    pool: &PortPool,
    latency_ms: Option<u32>,
) -> Result<MediaPlan> {
    let latency_ms = latency_ms.unwrap_or_else(|| transport.default_latency_ms());
    match transport {
        // The node listens, the core dials. That way round because a node is
        // often the machine behind the camera and the core is the one with the
        // stable address, and because one listener serves a core that
        // reconnects without the node having to be told.
        BridgeTransport::Srt => {
            let port = pool.take(1)?;
            Ok(MediaPlan {
                transport,
                target: format!("srt://{media_host}:{port}"),
                audio_port: 0,
                latency_ms,
            })
        }
        // The core listens on two UDP ports and the node sends to them. RTP is
        // the other way round from SRT because a jitter buffer belongs at the
        // receiver and the receiver is the core.
        BridgeTransport::Rtp => {
            let port = pool.take(2)?;
            Ok(MediaPlan {
                transport,
                target: format!("{core_host}:{port}"),
                audio_port: port + 1,
                latency_ms,
            })
        }
        // WHIP dials out of the node into whatever endpoint the operator
        // named, so no port is allocated here; the core receives it back
        // through a `whep` source.
        BridgeTransport::Whip => Ok(MediaPlan {
            transport,
            target: format!("http://{core_host}:8889/whip/{}", "node"),
            audio_port: 0,
            latency_ms,
        }),
    }
}

/// What the core builds to receive one remote source.
///
/// The elements come back unlinked and not in a pipeline, because the caller
/// adds them to the pipeline the mixer owns and then links them, which is the
/// same shape `SidecarSource::ingest` uses.
pub struct Receiver {
    /// Everything to add to the pipeline, in order.
    pub elements: Vec<gst::Element>,
    /// The element that produces sometimes pads, to route into the
    /// normaliser. Always a `decodebin`.
    pub decode: gst::Element,
}

/// Build the core's receive side for one plan.
pub fn receiver(id: &str, plan: &MediaPlan) -> Result<Receiver> {
    match plan.transport {
        BridgeTransport::Srt => srt_receiver(id, plan),
        BridgeTransport::Rtp => rtp_receiver(id, plan),
        BridgeTransport::Whip => whip_receiver(id, plan),
    }
}

fn srt_receiver(id: &str, plan: &MediaPlan) -> Result<Receiver> {
    needs(&["srtsrc"], "srt")?;
    let src = make("srtsrc", &format!("{id}-srtsrc"))?;
    // Caller mode: the node is listening. `latency` is the budget in
    // milliseconds and SRT negotiates the larger of the two peers' figures,
    // which is what 04 section 3 asks for.
    src.set_property(
        "uri",
        format!("{}?mode=caller&latency={}", plan.target, plan.latency_ms),
    );
    crate::probe::set_bool(&src, "wait-for-connection", false);
    let queue = make("queue", &format!("{id}-srt-q"))?;
    crate::probe::set_int(&queue, "max-size-time", plan.latency_ms as i64 * 1_000_000);
    let decode = make("decodebin", &format!("{id}-decode"))?;
    Ok(Receiver { elements: vec![src.clone(), queue.clone(), decode.clone()], decode })
}

fn rtp_receiver(id: &str, plan: &MediaPlan) -> Result<Receiver> {
    needs(&["rtpbin", "udpsrc", "rtpptdemux"], "rtp")?;
    let port: u16 = plan
        .target
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .context("an RTP plan whose target has no port")?;
    let bin = make("rtpbin", &format!("{id}-rtpbin"))?;
    // The three properties 04 section 4 names. `ntp-sync` makes the receiver
    // put buffers on the sender's NTP timeline, `ntp-time-source=clock-time`
    // says that timeline is the pipeline clock (which is the shared net clock),
    // and `latency` is the declared budget rather than a guess.
    crate::probe::set_bool(&bin, "ntp-sync", true);
    crate::probe::set_enum(&bin, "ntp-time-source", "clock-time");
    crate::probe::set_bool(&bin, "rfc7273-sync", true);
    crate::probe::set_int(&bin, "latency", plan.latency_ms as i64);
    crate::probe::set_enum(&bin, "buffer-mode", "synced");

    let video = make("udpsrc", &format!("{id}-rtp-v"))?;
    video.set_property("port", port as i32);
    video.set_property(
        "caps",
        gst::Caps::builder("application/x-rtp")
            .field("media", "video")
            .field("clock-rate", 90_000i32)
            .field("encoding-name", "H264")
            .build(),
    );
    let audio = make("udpsrc", &format!("{id}-rtp-a"))?;
    audio.set_property("port", plan.audio_port as i32);
    audio.set_property(
        "caps",
        gst::Caps::builder("application/x-rtp")
            .field("media", "audio")
            .field("clock-rate", 48_000i32)
            .field("encoding-name", "OPUS")
            .build(),
    );
    let decode = make("decodebin", &format!("{id}-decode"))?;
    Ok(Receiver {
        elements: vec![bin.clone(), video, audio, decode.clone()],
        decode,
    })
}

fn whip_receiver(id: &str, _plan: &MediaPlan) -> Result<Receiver> {
    needs(&["whipserversrc"], "whip")?;
    let src = make("whipserversrc", &format!("{id}-whip"))?;
    let decode = make("decodebin", &format!("{id}-decode"))?;
    Ok(Receiver { elements: vec![src, decode.clone()], decode })
}

/// Link a receiver's fixed elements to each other, and say which element the
/// sometimes pads come out of.
///
/// RTP is the odd one: `rtpbin` has request pads, so the two `udpsrc`s are
/// linked into it by name rather than in a chain.
pub fn link_receiver(rx: &Receiver, plan: &MediaPlan) -> Result<()> {
    match plan.transport {
        BridgeTransport::Srt => {
            let refs: Vec<&gst::Element> = rx.elements.iter().collect();
            gst::Element::link_many(&refs).context("linking the SRT receive chain")
        }
        BridgeTransport::Whip => {
            // `whipserversrc` has sometimes pads, one per track the browser or
            // the node offered, so there is nothing to link until the first
            // one turns up.
            let decode = rx.decode.clone();
            rx.elements[0].connect_pad_added(move |_, pad| link_into(pad, &decode, "WHIP"));
            Ok(())
        }
        BridgeTransport::Rtp => {
            let bin = &rx.elements[0];
            let video = &rx.elements[1];
            let audio = &rx.elements[2];
            let decode = rx.decode.clone();
            for (src, session) in [(video, 0u32), (audio, 1u32)] {
                let sink = bin
                    .request_pad_simple(&format!("recv_rtp_sink_{session}"))
                    .with_context(|| format!("rtpbin would not give a sink pad for session {session}"))?;
                let out = src.static_pad("src").context("a udpsrc with no src pad")?;
                out.link(&sink).with_context(|| {
                    format!("linking the RTP receive socket for session {session}")
                })?;
            }
            // `rtpbin` produces one sometimes pad per stream once the first
            // packet arrives; both go into the same decodebin, which sorts out
            // the payloads.
            bin.connect_pad_added(move |_, pad| link_into(pad, &decode, "RTP"));
            Ok(())
        }
    }
}

/// Put one sometimes pad into a decodebin, saying which transport it came from
/// when it will not go.
///
/// `decodebin` takes one stream on its static sink pad and more through
/// `sink_%u`, so the first arrival takes the static one and the rest request.
fn link_into(pad: &gst::Pad, decode: &gst::Element, what: &str) {
    let free = decode.static_pad("sink").filter(|p| !p.is_linked());
    let Some(sink) = free.or_else(|| decode.request_pad_simple("sink_%u")) else {
        tracing::warn!(transport = what, "decodebin would not take another stream");
        return;
    };
    if let Err(e) = pad.link(&sink) {
        tracing::warn!(transport = what, ?e, "linking a stream into decodebin");
    }
}

/// What a node builds to send one source to the core.
///
/// Returns a bin with a `video` and an `audio` ghost sink pad, ready for raw
/// media; the encoding happens inside, with the node's own catalogue choice.
pub fn sender(id: &str, plan: &MediaPlan, canvas: &CanvasCaps, encoder: &str) -> Result<gst::Bin> {
    let bin = gst::Bin::with_name(&format!("{id}-send"));
    match plan.transport {
        BridgeTransport::Srt => srt_sender(&bin, id, plan, canvas, encoder),
        BridgeTransport::Rtp => rtp_sender(&bin, id, plan, canvas, encoder),
        BridgeTransport::Whip => whip_sender(&bin, id, plan, canvas, encoder),
    }?;
    Ok(bin)
}

fn srt_sender(
    bin: &gst::Bin,
    id: &str,
    plan: &MediaPlan,
    canvas: &CanvasCaps,
    encoder: &str,
) -> Result<()> {
    needs(&["srtsink", "mpegtsmux"], "srt")?;
    let (venc, vpay) = video_chain(id, canvas, encoder)?;
    let (aenc, apay) = audio_chain(id, true)?;
    let mux = make("mpegtsmux", &format!("{id}-mux"))?;
    let sink = make("srtsink", &format!("{id}-srtsink"))?;
    let port = plan.target.rsplit(':').next().unwrap_or("0");
    sink.set_property(
        "uri",
        format!("srt://0.0.0.0:{port}?mode=listener&latency={}", plan.latency_ms),
    );
    crate::probe::set_bool(&sink, "wait-for-connection", false);
    crate::probe::set_bool(&sink, "async", false);
    let mut all = vec![mux.clone(), sink.clone()];
    all.extend(venc.clone());
    all.extend(vpay.clone());
    all.extend(aenc.clone());
    all.extend(apay.clone());
    bin.add_many(all.iter().collect::<Vec<_>>().as_slice())?;
    chain_into(bin, "video", &venc, &vpay, &mux, "sink_%d")?;
    chain_into(bin, "audio", &aenc, &apay, &mux, "sink_%d")?;
    gst::Element::link(&mux, &sink).context("linking the MPEG-TS mux to the SRT sink")?;
    Ok(())
}

fn rtp_sender(
    bin: &gst::Bin,
    id: &str,
    plan: &MediaPlan,
    canvas: &CanvasCaps,
    encoder: &str,
) -> Result<()> {
    needs(&["rtpbin", "udpsink", "rtph264pay", "rtpopuspay"], "rtp")?;
    let (host, port) = plan.target.rsplit_once(':').context("an RTP target with no port")?;
    let port: i32 = port.parse().context("an RTP target whose port is not a number")?;
    let rtp = make("rtpbin", &format!("{id}-rtpbin"))?;
    // The sender half of RFC 6051: every packet carries the sender's clock
    // time, so the receiver knows the timeline from the first packet rather
    // than waiting for an RTCP sender report.
    crate::probe::set_enum(&rtp, "ntp-time-source", "clock-time");
    crate::probe::set_bool(&rtp, "add-reference-timestamp-meta", true);
    crate::probe::set_enum(&rtp, "rtp-profile", "avpf");

    let (venc, vpay) = video_chain(id, canvas, encoder)?;
    let (aenc, apay) = audio_chain(id, false)?;
    let vsink = make("udpsink", &format!("{id}-udp-v"))?;
    vsink.set_property("host", host);
    vsink.set_property("port", port);
    crate::probe::set_bool(&vsink, "async", false);
    crate::probe::set_bool(&vsink, "sync", true);
    let asink = make("udpsink", &format!("{id}-udp-a"))?;
    asink.set_property("host", host);
    asink.set_property("port", plan.audio_port as i32);
    crate::probe::set_bool(&asink, "async", false);
    crate::probe::set_bool(&asink, "sync", true);

    let mut all = vec![rtp.clone(), vsink.clone(), asink.clone()];
    all.extend(venc.clone());
    all.extend(vpay.clone());
    all.extend(aenc.clone());
    all.extend(apay.clone());
    bin.add_many(all.iter().collect::<Vec<_>>().as_slice())?;

    for (session, enc, pay, sink, name) in [
        (0u32, &venc, &vpay, &vsink, "video"),
        (1u32, &aenc, &apay, &asink, "audio"),
    ] {
        let mut chain: Vec<&gst::Element> = enc.iter().collect();
        chain.extend(pay.iter());
        gst::Element::link_many(&chain).with_context(|| format!("linking the {name} RTP chain"))?;
        let head = chain.first().context("an empty RTP chain")?;
        ghost(bin, name, head)?;
        let tail = chain.last().context("an empty RTP chain")?;
        let send = rtp
            .request_pad_simple(&format!("send_rtp_sink_{session}"))
            .context("rtpbin would not give a send pad")?;
        tail.static_pad("src")
            .context("a payloader with no src pad")?
            .link(&send)
            .with_context(|| format!("linking the {name} payloader into rtpbin"))?;
        let out = rtp
            .static_pad(&format!("send_rtp_src_{session}"))
            .context("rtpbin has no send_rtp_src pad yet")?;
        out.link(&sink.static_pad("sink").context("a udpsink with no sink pad")?)
            .with_context(|| format!("linking rtpbin to the {name} socket"))?;
    }
    Ok(())
}

fn whip_sender(
    bin: &gst::Bin,
    id: &str,
    plan: &MediaPlan,
    canvas: &CanvasCaps,
    encoder: &str,
) -> Result<()> {
    needs(&["whipclientsink"], "whip")?;
    let (venc, _) = video_chain(id, canvas, encoder)?;
    let (aenc, _) = audio_chain(id, false)?;
    let sink = make("whipclientsink", &format!("{id}-whip"))?;
    sink.set_property("signaller::whip-endpoint", &plan.target);
    let mut all = vec![sink.clone()];
    all.extend(venc.clone());
    all.extend(aenc.clone());
    bin.add_many(all.iter().collect::<Vec<_>>().as_slice())?;
    for (chain, name) in [(&venc, "video"), (&aenc, "audio")] {
        let refs: Vec<&gst::Element> = chain.iter().collect();
        gst::Element::link_many(&refs).with_context(|| format!("linking the {name} chain"))?;
        ghost(bin, name, refs[0])?;
        let tail = refs.last().context("an empty chain")?;
        let pad = sink
            .request_pad_simple("sink_%u")
            .context("whipclientsink would not give a sink pad")?;
        tail.static_pad("src")
            .context("no src pad")?
            .link(&pad)
            .with_context(|| format!("linking {name} into whipclientsink"))?;
    }
    Ok(())
}

/// Encode raw video with whatever the node's own probe chose.
fn video_chain(
    id: &str,
    canvas: &CanvasCaps,
    encoder: &str,
) -> Result<(Vec<gst::Element>, Vec<gst::Element>)> {
    let convert = make("videoconvert", &format!("{id}-vconv"))?;
    let enc = make(encoder, &format!("{id}-venc"))?;
    // Short GOP and no B frames: a remote source is a live feed and the person
    // watching cares about the delay, not the bitrate.
    crate::probe::set_int(&enc, "key-int-max", canvas.fps.numer() as i64 * 2);
    crate::probe::set_int(&enc, "bframes", 0);
    crate::probe::set_enum(&enc, "tune", "zerolatency");
    let parse = make("h264parse", &format!("{id}-vparse"))?;
    crate::probe::set_int(&parse, "config-interval", -1);
    let pay = if crate::probe::exists("rtph264pay") {
        vec![make("rtph264pay", &format!("{id}-vpay"))?]
    } else {
        Vec::new()
    };
    if let Some(p) = pay.first() {
        crate::probe::set_int(p, "config-interval", -1);
        crate::probe::set_int(p, "pt", 96);
    }
    Ok((vec![convert, enc, parse], pay))
}

fn audio_chain(id: &str, aac: bool) -> Result<(Vec<gst::Element>, Vec<gst::Element>)> {
    let convert = make("audioconvert", &format!("{id}-aconv"))?;
    let resample = make("audioresample", &format!("{id}-ares"))?;
    // MPEG-TS wants AAC; RTP and WebRTC want Opus. Both are in the same base
    // install, so this is a choice rather than a dependency.
    let (factory, parser) = if aac { ("avenc_aac", Some("aacparse")) } else { ("opusenc", None) };
    let enc = make(factory, &format!("{id}-aenc"))?;
    let mut chain = vec![convert, resample, enc];
    if let Some(p) = parser {
        chain.push(make(p, &format!("{id}-aparse"))?);
    }
    let pay = if aac || !crate::probe::exists("rtpopuspay") {
        Vec::new()
    } else {
        let p = make("rtpopuspay", &format!("{id}-apay"))?;
        crate::probe::set_int(&p, "pt", 97);
        vec![p]
    };
    Ok((chain, pay))
}

/// Link a chain, ghost its head onto the bin under `name`, and put its tail
/// into a muxer's request pad.
fn chain_into(
    bin: &gst::Bin,
    name: &str,
    enc: &[gst::Element],
    pay: &[gst::Element],
    mux: &gst::Element,
    pad_template: &str,
) -> Result<()> {
    let mut chain: Vec<&gst::Element> = enc.iter().collect();
    chain.extend(pay.iter());
    gst::Element::link_many(&chain).with_context(|| format!("linking the {name} chain"))?;
    let head = chain.first().context("an empty chain")?;
    ghost(bin, name, head)?;
    let tail = chain.last().context("an empty chain")?;
    let sink = mux
        .request_pad_simple(pad_template)
        .with_context(|| format!("the muxer would not give a pad for {name}"))?;
    tail.static_pad("src")
        .context("no src pad")?
        .link(&sink)
        .with_context(|| format!("linking {name} into the muxer"))?;
    Ok(())
}

fn ghost(bin: &gst::Bin, name: &str, head: &gst::Element) -> Result<()> {
    let pad = head.static_pad("sink").context("a chain head with no sink pad")?;
    let ghost = gst::GhostPad::with_target(&pad).context("make a ghost pad")?;
    ghost.set_property("name", name);
    bin.add_pad(&ghost).with_context(|| format!("adding the {name} pad to the send bin"))?;
    Ok(())
}

/// Refuse early and by name when the elements a transport needs are not here.
fn needs(elements: &[&str], transport: &str) -> Result<()> {
    // Asked by building one, not by looking it up. A factory can be in the
    // registry and still fail every `make`, because the registry remembers a
    // plugin file that will not load: the official Windows GStreamer 1.28
    // installer ships `gstsrt.dll` in exactly that state, and `srtsrc` there
    // answers `exists` with yes and `make` with "Failed to load element
    // factory". An element the bridge cannot build is an element this machine
    // has not got, and the operator needs to be told that here, with the
    // other transport named, rather than three lines later.
    let missing: Vec<&str> = elements
        .iter()
        .copied()
        .filter(|e| gst::ElementFactory::make(e).build().is_err())
        .collect();
    anyhow::ensure!(
        missing.is_empty(),
        "this machine cannot build {} for the `{transport}` transport between a node and \
         the core. Install gst-plugins-bad, or set transport = \"{}\" on the source",
        missing.join(", "),
        if transport == "srt" { "rtp" } else { "srt" }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pool_hands_out_distinct_ports() {
        let pool = PortPool::new();
        let a = pool.take(2).unwrap();
        let b = pool.take(2).unwrap();
        assert_ne!(a, b);
        assert!(b >= a + 2 || a >= b + 2, "two takes must not overlap: {a} and {b}");
        pool.give_back(a, 2);
        pool.give_back(b, 2);
    }

    #[test]
    fn a_plan_says_where_the_media_goes() {
        let pool = PortPool::new();
        let srt = plan(BridgeTransport::Srt, "10.0.0.21", "10.0.0.1", &pool, None).unwrap();
        assert!(srt.target.starts_with("srt://10.0.0.21:"), "got {}", srt.target);
        assert_eq!(srt.latency_ms, 120);

        let rtp = plan(BridgeTransport::Rtp, "10.0.0.21", "10.0.0.1", &pool, Some(150)).unwrap();
        assert!(rtp.target.starts_with("10.0.0.1:"), "got {}", rtp.target);
        assert_eq!(rtp.latency_ms, 150);
        assert_eq!(
            rtp.audio_port,
            rtp.target.rsplit(':').next().unwrap().parse::<u16>().unwrap() + 1,
            "RTP audio takes the port after video"
        );
    }

    #[test]
    fn the_core_can_build_a_receiver_for_every_transport_it_has_elements_for() {
        gst::init().unwrap();
        let pool = PortPool::new();
        for transport in BridgeTransport::ALL {
            let p = plan(transport, "127.0.0.1", "127.0.0.1", &pool, None).unwrap();
            match receiver("test", &p) {
                Ok(rx) => {
                    assert!(!rx.elements.is_empty());
                    let pipeline = gst::Pipeline::new();
                    pipeline
                        .add_many(rx.elements.iter().collect::<Vec<_>>().as_slice())
                        .unwrap();
                    link_receiver(&rx, &p)
                        .unwrap_or_else(|e| panic!("{} receiver would not link: {e:#}", transport.as_str()));
                }
                Err(e) => {
                    // A machine without gst-plugins-bad is a real machine. The
                    // refusal has to name what is missing and what to do.
                    let text = format!("{e:#}");
                    assert!(text.contains("transport"), "an unhelpful refusal: {text}");
                }
            }
        }
    }
}
