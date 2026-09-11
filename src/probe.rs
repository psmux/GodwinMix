//! Runtime codec backend selection.
//!
//! The same binary has to run on an NVIDIA server, an Intel box with a VA
//! capable iGPU, a Windows workstation, a Mac, and a rented VM with no GPU at
//! all. Nothing above this module knows which of those it is on. We probe the
//! registry once at startup, pick the best available decoder and encoder
//! independently, and hand the rest of the program a pair of element names.
//!
//! Two rules keep this honest:
//!
//! 1. Decode and encode are chosen separately. A machine with NVDEC but no
//!    NVENC licence is a real configuration and it should use both the fast
//!    decoder and the software encoder.
//! 2. Every property is set defensively. Backends disagree about names,
//!    units and integer widths, and a property that does not exist on this
//!    version of this plugin must be a logged warning, never a panic. A mixer
//!    that refuses to start because it could not set `rc-lookahead` is worse
//!    than one that runs with the default.

use crate::config::Accel;
use anyhow::{bail, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::{debug, info, warn};

/// A decoder choice, plus the element needed to pull frames back into system
/// memory. Hardware decoders hand out GPU surfaces; the mixer works in system
/// memory, so a download step is required for most accelerated backends.
#[derive(Debug, Clone, Copy)]
pub struct DecoderChoice {
    pub accel: Accel,
    pub element: &'static str,
    pub download: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub struct EncoderChoice {
    pub accel: Accel,
    pub element: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct Backends {
    pub video_decode: DecoderChoice,
    pub video_encode: EncoderChoice,
    pub audio_decode: &'static str,
    pub audio_encode: &'static str,
}

/// Preference order, best first. Hardware before software within each family.
const VIDEO_DECODERS: &[DecoderChoice] = &[
    DecoderChoice { accel: Accel::Nvidia, element: "nvh264dec", download: Some("cudadownload") },
    DecoderChoice { accel: Accel::Nvidia, element: "nvh264sldec", download: Some("cudadownload") },
    DecoderChoice { accel: Accel::Va, element: "vah264dec", download: Some("vapostproc") },
    DecoderChoice { accel: Accel::Va, element: "vaapih264dec", download: None },
    DecoderChoice { accel: Accel::D3d11, element: "d3d11h264dec", download: Some("d3d11download") },
    DecoderChoice { accel: Accel::VideoToolbox, element: "vtdec_hw", download: None },
    DecoderChoice { accel: Accel::Software, element: "avdec_h264", download: None },
    DecoderChoice { accel: Accel::Software, element: "openh264dec", download: None },
];

const VIDEO_ENCODERS: &[EncoderChoice] = &[
    EncoderChoice { accel: Accel::Nvidia, element: "nvh264enc" },
    EncoderChoice { accel: Accel::Va, element: "vah264enc" },
    EncoderChoice { accel: Accel::Va, element: "vaapih264enc" },
    EncoderChoice { accel: Accel::MediaFoundation, element: "mfh264enc" },
    EncoderChoice { accel: Accel::VideoToolbox, element: "vtenc_h264_hw" },
    EncoderChoice { accel: Accel::VideoToolbox, element: "vtenc_h264" },
    EncoderChoice { accel: Accel::Software, element: "x264enc" },
];

const AUDIO_DECODERS: &[&str] = &["avdec_aac", "faad"];
const AUDIO_ENCODERS: &[&str] = &["fdkaacenc", "avenc_aac", "voaacenc"];

/// The best AAC encoder installed, or None. Split out of `Backends::probe` so
/// the file converter and the programme encoder cannot drift onto different
/// lists.
pub fn best_audio_encoder() -> Option<&'static str> {
    AUDIO_ENCODERS.iter().copied().find(|f| exists(f))
}

pub fn exists(factory: &str) -> bool {
    gst::ElementFactory::find(factory).is_some()
}

impl Backends {
    pub fn probe(decode_pref: Accel, encode_pref: Accel) -> Result<Self> {
        let video_decode = pick_decoder(decode_pref)?;
        let video_encode = pick_encoder(encode_pref)?;

        let audio_decode = AUDIO_DECODERS
            .iter()
            .copied()
            .find(|f| exists(f))
            .ok_or_else(|| anyhow::anyhow!("no AAC decoder available (tried {AUDIO_DECODERS:?})"))?;
        let audio_encode = best_audio_encoder()
            .ok_or_else(|| anyhow::anyhow!("no AAC encoder available (tried {AUDIO_ENCODERS:?})"))?;

        let b = Self { video_decode, video_encode, audio_decode, audio_encode };
        info!(
            decoder = b.video_decode.element,
            decode_accel = ?b.video_decode.accel,
            encoder = b.video_encode.element,
            encode_accel = ?b.video_encode.accel,
            audio_decoder = b.audio_decode,
            audio_encoder = b.audio_encode,
            "selected codec backends"
        );
        if b.video_encode.accel == Accel::Software {
            warn!("using software H.264 encoding; expect roughly one core per 1080p30 output");
        }
        Ok(b)
    }

    /// Raise the rank of the chosen decoder so that `decodebin`, and anything
    /// built on it such as `fallbacksrc`, converges on the same choice we made
    /// here. Without this the auto-plugger picks by its own ranking and you end
    /// up with software decode on a machine that has a perfectly good GPU.
    pub fn apply_decoder_ranks(&self) {
        if let Some(f) = gst::ElementFactory::find(self.video_decode.element) {
            f.set_rank(gst::Rank::PRIMARY + 256);
            debug!(element = self.video_decode.element, "raised decoder rank for autoplugging");
        }
        if let Some(f) = gst::ElementFactory::find(self.audio_decode) {
            f.set_rank(gst::Rank::PRIMARY + 256);
        }
    }
}

fn pick_decoder(pref: Accel) -> Result<DecoderChoice> {
    let mut candidates = VIDEO_DECODERS.iter().filter(|d| exists(d.element));
    match pref {
        Accel::Auto => candidates
            .next()
            .copied()
            .ok_or_else(|| anyhow::anyhow!("no H.264 decoder available at all")),
        want => match candidates.find(|d| d.accel == want) {
            Some(d) => Ok(*d),
            None => bail!("hardware.decode = {want:?} was requested but no matching decoder is installed"),
        },
    }
}

fn pick_encoder(pref: Accel) -> Result<EncoderChoice> {
    let mut candidates = VIDEO_ENCODERS.iter().filter(|e| exists(e.element));
    match pref {
        Accel::Auto => candidates
            .next()
            .copied()
            .ok_or_else(|| anyhow::anyhow!("no H.264 encoder available at all")),
        want => match candidates.find(|e| e.accel == want) {
            Some(e) => Ok(*e),
            None => bail!("hardware.encode = {want:?} was requested but no matching encoder is installed"),
        },
    }
}

// ---------------------------------------------------------------------------
// Defensive property setting
// ---------------------------------------------------------------------------

/// Look up a property that we are allowed to set right now.
///
/// Returns None both when the property does not exist on this backend and when
/// it exists but cannot be written after construction. GStreamer panics on a
/// write to a construct-only property, and `force-live` on the aggregators is
/// exactly that, so this check is not theoretical.
fn writable_property(el: &gst::Element, prop: &str) -> Option<glib::ParamSpec> {
    let pspec = el.find_property(prop).or_else(|| {
        debug!(element = %el.name(), prop, "property not present on this backend, skipping");
        None
    })?;
    let flags = pspec.flags();
    if !flags.contains(glib::ParamFlags::WRITABLE) {
        debug!(element = %el.name(), prop, "property is read-only, skipping");
        return None;
    }
    if flags.contains(glib::ParamFlags::CONSTRUCT_ONLY) {
        warn!(
            element = %el.name(),
            prop,
            "property can only be set at construction; use a builder instead"
        );
        return None;
    }
    Some(pspec)
}

/// Set an integer-valued property regardless of the width the backend chose
/// for it. `bitrate` is guint on x264enc, gint on vtenc_h264 and guint on
/// nvh264enc; without this every backend would need its own call site.
pub fn set_int(el: &gst::Element, prop: &str, v: i64) {
    let Some(pspec) = writable_property(el, prop) else {
        return;
    };
    let t = pspec.value_type();
    if t == u32::static_type() {
        el.set_property(prop, v.max(0) as u32);
    } else if t == i32::static_type() {
        el.set_property(prop, v as i32);
    } else if t == u64::static_type() {
        el.set_property(prop, v.max(0) as u64);
    } else if t == i64::static_type() {
        el.set_property(prop, v);
    } else if t == f64::static_type() {
        el.set_property(prop, v as f64);
    } else {
        warn!(element = %el.name(), prop, ?t, "unexpected property type, skipping");
    }
}

pub fn set_bool(el: &gst::Element, prop: &str, v: bool) {
    if writable_property(el, prop).is_some() {
        el.set_property(prop, v);
    }
}

/// Set an enum or flags property by its nickname. Nicknames are stable across
/// plugin versions in a way that numeric enum values are not.
pub fn set_enum(el: &gst::Element, prop: &str, nick: &str) {
    if writable_property(el, prop).is_some() {
        el.set_property_from_str(prop, nick);
    }
}

/// Apply low latency, constant bitrate, no B-frame settings appropriate to the
/// selected backend.
///
/// B-frames are disabled everywhere. They cost latency and buy very little at
/// live streaming bitrates, and reordering makes a mid-stream splice harder to
/// reason about.
pub fn configure_video_encoder(
    el: &gst::Element,
    accel: Accel,
    bitrate_kbps: u32,
    keyframe_frames: u32,
) {
    match accel {
        Accel::Software => {
            set_int(el, "bitrate", bitrate_kbps as i64);
            set_int(el, "key-int-max", keyframe_frames as i64);
            set_int(el, "bframes", 0);
            set_enum(el, "tune", "zerolatency");
            set_enum(el, "speed-preset", "veryfast");
            set_enum(el, "pass", "cbr");
            set_bool(el, "aud", false);
            // flvmux wants AVC sample format, not Annex B byte stream.
            set_bool(el, "byte-stream", false);
        }
        Accel::Nvidia => {
            set_int(el, "bitrate", bitrate_kbps as i64);
            set_int(el, "max-bitrate", bitrate_kbps as i64);
            set_int(el, "gop-size", keyframe_frames as i64);
            set_int(el, "bframes", 0);
            set_int(el, "b-frames", 0);
            set_enum(el, "rc-mode", "cbr");
            set_enum(el, "preset", "low-latency-hq");
            set_bool(el, "zerolatency", true);
        }
        Accel::Va => {
            set_int(el, "bitrate", bitrate_kbps as i64);
            set_int(el, "key-int-max", keyframe_frames as i64);
            set_int(el, "b-frames", 0);
            set_enum(el, "rate-control", "cbr");
        }
        Accel::VideoToolbox => {
            set_int(el, "bitrate", bitrate_kbps as i64);
            set_int(el, "max-keyframe-interval", keyframe_frames as i64);
            set_bool(el, "realtime", true);
            set_bool(el, "allow-frame-reordering", false);
            set_enum(el, "rate-control", "cbr");
        }
        Accel::MediaFoundation | Accel::D3d11 => {
            set_int(el, "bitrate", bitrate_kbps as i64);
            set_int(el, "gop-size", keyframe_frames as i64);
            set_int(el, "bframes", 0);
            set_enum(el, "rc-mode", "cbr");
            set_bool(el, "low-latency", true);
        }
        Accel::Auto => unreachable!("Auto is resolved to a concrete backend during probe"),
    }
}

/// AAC encoders take bitrate in bits per second, unlike every video encoder
/// here, which takes kilobits. Getting this wrong gives you either a 160 bit/s
/// stream or a 160 Mbit/s one, and both fail in confusing ways.
/// Samples of encoder delay the AAC encoder does not take out of its
/// timestamps, so audio leaves it that much late relative to video.
///
/// Measured with `browser/dev/tail-offset.sh`: a file whose flash and beep
/// coincide comes out of the programme tail with the beep 43 ms late through
/// fdkaacenc and 21 ms late through avenc_aac, whatever the video encoder,
/// with or without the mixers in the path. Those are the two encoders'
/// documented priming delays (2048 and 1024 samples at 48 kHz). voaacenc is
/// assumed to behave like a standard AAC-LC encoder; it was not measured.
pub fn audio_encoder_delay_samples(element: &str) -> u64 {
    match element {
        "fdkaacenc" => 2048,
        "avenc_aac" | "voaacenc" => 1024,
        _ => 0,
    }
}

pub fn configure_audio_encoder(el: &gst::Element, bitrate_kbps: u32) {
    set_int(el, "bitrate", (bitrate_kbps as i64) * 1000);
    set_enum(el, "rate-control", "cbr");
    set_bool(el, "perfect-timestamp", true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        let _ = gst::init();
    }

    #[test]
    fn probing_auto_always_finds_something() {
        init();
        let b = Backends::probe(Accel::Auto, Accel::Auto).expect("some backend must exist");
        assert!(exists(b.video_decode.element));
        assert!(exists(b.video_encode.element));
        assert!(exists(b.audio_decode));
        assert!(exists(b.audio_encode));
    }

    #[test]
    fn requesting_absent_hardware_fails_loudly() {
        init();
        // No machine has every backend, so at least one forced request must
        // fail. Assert on whichever is genuinely missing here.
        let all = [Accel::Nvidia, Accel::Va, Accel::VideoToolbox, Accel::D3d11];
        let missing: Vec<_> = all
            .into_iter()
            .filter(|a| !VIDEO_DECODERS.iter().any(|d| d.accel == *a && exists(d.element)))
            .collect();
        for a in missing {
            assert!(pick_decoder(a).is_err(), "{a:?} should have been reported absent");
        }
    }

    #[test]
    fn software_is_always_available_as_a_floor() {
        init();
        assert!(pick_decoder(Accel::Software).is_ok());
        assert!(pick_encoder(Accel::Software).is_ok());
    }

    #[test]
    fn defensive_setters_ignore_unknown_properties() {
        init();
        let el = gst::ElementFactory::make("queue").build().unwrap();
        // Must not panic even though queue has none of these.
        set_int(&el, "bitrate", 4000);
        set_bool(&el, "realtime", true);
        set_enum(&el, "rc-mode", "cbr");
        // And a real property must still take effect, coerced to the right width.
        set_int(&el, "max-size-buffers", 42);
        assert_eq!(el.property::<u32>("max-size-buffers"), 42);
    }

    #[test]
    fn defensive_setters_refuse_construct_only_properties() {
        init();
        // `force-live` on the aggregators is construct-only. Writing it after
        // the fact panics inside GLib, so the setter must decline instead.
        let el = gst::ElementFactory::make("compositor").build().unwrap();
        assert!(el.find_property("force-live").is_some());
        assert!(writable_property(&el, "force-live").is_none());
        set_bool(&el, "force-live", true);
        assert!(!el.property::<bool>("force-live"), "value should not have changed");

        // A construct-time builder is the supported way to set it.
        let live = crate::gstutil::make_live_aggregator("compositor", "c").unwrap();
        assert!(live.property::<bool>("force-live"));
    }

    #[test]
    fn defensive_setters_refuse_read_only_properties() {
        init();
        let q = gst::ElementFactory::make("queue").build().unwrap();
        // current-level-time is readable but not writable.
        assert!(q.find_property("current-level-time").is_some());
        assert!(writable_property(&q, "current-level-time").is_none());
        set_int(&q, "current-level-time", 5);
    }
}
