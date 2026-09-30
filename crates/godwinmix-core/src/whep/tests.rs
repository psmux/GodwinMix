//! A `whep/output` fed the way the core feeds it (encoded H.264 and AAC on two
//! queues) and watched by a real WebRTC receiver: a second `webrtcbin` in this
//! process that makes the offer a browser makes, decodes what arrives and
//! counts it. No mocks: ICE, DTLS and SRTP all run.

use crate::config::OutputConfig;
use crate::plugin::output::{Output, OutputCtx};
use crate::plugin::Hello;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_webrtc as gst_webrtc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The programme side: live H.264 and AAC on two queues, and the output built
/// on them.
fn programme(id: &str) -> Option<(gst::Pipeline, Box<dyn Output>)> {
    gst::init().ok()?;
    for e in ["webrtcbin", "nicesrc", "x264enc", "avenc_aac", "opusenc", "avdec_h264"] {
        if !crate::probe::exists(e) {
            eprintln!("skipping: {e} is not installed");
            return None;
        }
    }
    let pipeline = gst::parse::launch(
        "videotestsrc is-live=true pattern=ball ! video/x-raw,width=320,height=240,framerate=30/1 \
         ! x264enc tune=zerolatency key-int-max=30 bframes=0 ! h264parse ! queue name=v \
         audiotestsrc is-live=true ! avenc_aac ! aacparse ! queue name=a",
    )
    .ok()?
    .downcast::<gst::Pipeline>()
    .ok()?;
    let cfg = OutputConfig::bare(id, "");
    let mut params = crate::config::Params::new();
    params.insert("stun".into(), toml::Value::String(String::new()));
    let mut out = match crate::plugin::output::by_type("whep/output") {
        Some(p) => (p.make)(&cfg).ok()?,
        None => return None,
    };
    let hello = Hello {
        instance: id.into(),
        canvas: crate::caps::CanvasCaps::new(&crate::config::Canvas::default()),
        api_level: crate::plugin::API_LEVEL,
        params: params.clone(),
        tier: crate::plugin::Tier::Core,
    };
    out.initialize(hello).expect("the output initialises");
    let ctx = OutputCtx { id, generation: 1, pipeline: &pipeline, params: &params, cfg: &cfg, taps: &[] };
    let (v, a) = (pipeline.by_name("v")?, pipeline.by_name("a")?);
    out.build(&ctx, &v, &a).expect("the output builds");
    pipeline.set_state(gst::State::Playing).ok()?;
    Some((pipeline, out))
}

/// A viewer: recvonly video and audio, decoded and counted.
struct Viewer {
    pipeline: gst::Pipeline,
    bin: gst::Element,
    frames: Arc<AtomicU64>,
    sound: Arc<AtomicU64>,
}

fn viewer(video: &str) -> Viewer {
    let pipeline = gst::Pipeline::new();
    let bin = gst::ElementFactory::make("webrtcbin").property_from_str("bundle-policy", "max-bundle").build().unwrap();
    pipeline.add(&bin).unwrap();
    for caps in [format!("application/x-rtp,media=video,encoding-name={video},clock-rate=90000,payload=102,packetization-mode=(string)1"), "application/x-rtp,media=audio,encoding-name=OPUS,clock-rate=48000,payload=111".into()] {
        let caps: gst::Caps = caps.parse().unwrap();
        let recv = gst_webrtc::WebRTCRTPTransceiverDirection::Recvonly;
        bin.emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>("add-transceiver", &[&recv, &caps]);
    }
    let (frames, sound) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    let (weak, f, s) = (pipeline.downgrade(), frames.clone(), sound.clone());
    bin.connect_pad_added(move |_, pad| {
        let Some(p) = weak.upgrade() else { return };
        // Software decoders named outright: a test thread has no main loop
        // for the platform's GL backed decoder to hand frames through.
        let caps = pad.current_caps().or_else(|| Some(pad.query_caps(None))).unwrap();
        let encoding = caps.structure(0).and_then(|s| s.get::<String>("encoding-name").ok()).unwrap_or_default();
        let (chain, count) = match encoding.as_str() {
            "H264" => ("rtph264depay ! h264parse ! avdec_h264", f.clone()),
            "OPUS" => ("rtpopusdepay ! opusdec", s.clone()),
            _ => return,
        };
        let bin = gst::parse::bin_from_description(&format!("{chain} ! fakesink sync=false name=end"), true).unwrap();
        p.add(&bin).unwrap();
        bin.sync_state_with_parent().unwrap();
        pad.link(&bin.static_pad("sink").unwrap()).unwrap();
        let end = bin.by_name("end").unwrap().static_pad("sink").unwrap();
        end.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    });
    pipeline.set_state(gst::State::Playing).unwrap();
    Viewer { pipeline, bin, frames, sound }
}

impl Viewer {
    fn offer(&self) -> String {
        let promise = gst::Promise::new();
        self.bin.emit_by_name::<()>("create-offer", &[&None::<gst::Structure>, &promise]);
        promise.wait();
        let offer = promise.get_reply().unwrap().get::<gst_webrtc::WebRTCSessionDescription>("offer").unwrap();
        self.bin.emit_by_name::<()>("set-local-description", &[&offer, &None::<gst::Promise>]);
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until && self.bin.property::<gst_webrtc::WebRTCICEGatheringState>("ice-gathering-state") != gst_webrtc::WebRTCICEGatheringState::Complete {
            std::thread::sleep(Duration::from_millis(20));
        }
        let local = self.bin.property::<Option<gst_webrtc::WebRTCSessionDescription>>("local-description").unwrap();
        local.sdp().as_text().unwrap()
    }

    fn answer(&self, sdp: &str) {
        let message = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes()).expect("the answer is SDP");
        let answer = gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Answer, message);
        let promise = gst::Promise::new();
        self.bin.emit_by_name::<()>("set-remote-description", &[&answer, &promise]);
        promise.wait();
    }

    fn wait_for(&self, frames: u64, sound: u64, secs: u64) -> (u64, u64) {
        let until = Instant::now() + Duration::from_secs(secs);
        loop {
            let got = (self.frames.load(Ordering::Relaxed), self.sound.load(Ordering::Relaxed));
            if (got.0 >= frames && got.1 >= sound) || Instant::now() > until {
                return got;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[test]
fn a_webrtc_viewer_decodes_the_programme_picture_and_sound() {
    let Some((pipeline, out)) = programme("whep-test-watch") else { return };
    let v = viewer("H264");
    let (_, session, answer) = super::offer("whep-test-watch", &v.offer()).expect("the offer is answered");
    assert!(answer.contains("a=sendonly"), "the answer should only send:\n{answer}");
    v.answer(&answer);
    let (frames, sound) = v.wait_for(60, 25, 15);
    assert!(frames >= 60, "the viewer decoded {frames} frames in 15 s; wanted 60");
    assert!(sound >= 25, "the viewer decoded {sound} Opus packets in 15 s; wanted 25");
    assert!(super::end("whep-test-watch", &session), "DELETE ends the session");
    assert!(!super::end("whep-test-watch", &session), "and only once");
    drop(v);
    drop(out);
    assert!(!super::ids().contains(&"whep-test-watch".to_string()), "a dropped output is withdrawn");
    let _ = pipeline.set_state(gst::State::Null);
}

#[test]
fn an_offer_without_the_codec_sent_is_refused_with_406_and_the_codec_named() {
    let Some((pipeline, _out)) = programme("whep-test-vp8") else { return };
    let v = viewer("VP8");
    let err = super::offer("whep-test-vp8", &v.offer()).expect_err("VP8 only cannot take H.264");
    assert_eq!(err.status, 406);
    assert!(err.message.contains("H264"), "{}", err.message);
    let missing = super::offer("whep-no-such-output", "v=0").expect_err("no output by that name");
    assert_eq!(missing.status, 404);
    assert!(missing.message.contains("Outputs"), "{}", missing.message);
    let _ = pipeline.set_state(gst::State::Null);
}
