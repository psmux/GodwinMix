//! Runtime codec backend selection, and the defensive property setters.
//!
//! The same binary has to run on an NVIDIA server, an Intel box with a VA
//! capable iGPU, a Windows workstation, a Mac, a Raspberry Pi and a rented VM
//! with no GPU at all. Nothing above this module knows which of those it is
//! on. The registry is probed once at startup, the best available decoder and
//! encoder are picked independently, and the rest of the program gets a pair
//! of element names.
//!
//! What used to be four static tables in this file is now `codecs.toml`, read
//! by `crate::catalogue`. A new GPU generation or a renamed element is an
//! entry somebody sends rather than a core release. This module is what is
//! left: the shape the rest of the code already asks for, and the property
//! setters that keep a missing property a warning instead of a panic.
//!
//! Two rules keep it honest:
//!
//! 1. Decode and encode are chosen separately. A machine with NVDEC but no
//!    NVENC licence is a real configuration and it should use both the fast
//!    decoder and the software encoder.
//! 2. Every property is set defensively. Backends disagree about names, units
//!    and integer widths, and a property that does not exist on this version
//!    of this plugin must be a logged warning, never a panic. A mixer that
//!    refuses to start because it could not set `rc-lookahead` is worse than
//!    one that runs with the default.

use crate::catalogue;
use crate::catalogue::select::{GstRegistry, Request, Selection};
use crate::config::Accel;
use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::collections::BTreeSet;
use tracing::{debug, warn};

/// A decoder choice, plus the element needed to pull frames back into system
/// memory. Hardware decoders hand out GPU surfaces; the source path works in
/// system memory, so a download step is required for most accelerated
/// backends.
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

/// Catalogue entries are read from a file at runtime, but the rest of the code
/// has always held element names as `&'static str` and they live for the whole
/// process anyway. Interning gives back the `'static` without leaking a fresh
/// copy every time something asks: the set is a few dozen names and never
/// grows past the size of the catalogue.
pub fn intern(s: &str) -> &'static str {
    static NAMES: std::sync::OnceLock<parking_lot::Mutex<BTreeSet<&'static str>>> =
        std::sync::OnceLock::new();
    let names = NAMES.get_or_init(|| parking_lot::Mutex::new(BTreeSet::new()));
    let mut guard = names.lock();
    if let Some(found) = guard.get(s) {
        return found;
    }
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    guard.insert(leaked);
    leaked
}

fn accel_of(name: &str) -> Accel {
    match name {
        "nvidia" => Accel::Nvidia,
        "va" => Accel::Va,
        "qsv" => Accel::Qsv,
        "amf" => Accel::Amf,
        "videotoolbox" => Accel::VideoToolbox,
        "mediafoundation" => Accel::MediaFoundation,
        "d3d11" => Accel::D3d11,
        "d3d12" => Accel::D3d12,
        "cuda" => Accel::Cuda,
        "gl" => Accel::Gl,
        "vulkan" => Accel::Vulkan,
        "v4l2" => Accel::V4l2,
        "software" => Accel::Software,
        other => {
            // A catalogue the operator extended with a vendor this build has
            // no variant for. It still gets selected by rank; it just cannot
            // be pinned by name from `[hardware]`.
            debug!(accel = other, "catalogue accel has no config variant; treated as auto");
            Accel::Auto
        }
    }
}

pub fn exists(factory: &str) -> bool {
    gst::ElementFactory::find(factory).is_some()
}

/// The best AAC encoder installed, or None. Split out so the file converter
/// and the programme encoder cannot drift onto different lists.
pub fn best_audio_encoder() -> Option<&'static str> {
    let cat = catalogue::global();
    let mut best: Option<(i32, &str)> = None;
    for e in &cat.audio {
        if e.disabled || e.codec != "aac" {
            continue;
        }
        let Some(enc) = e.encoder.as_deref() else { continue };
        if !exists(enc) {
            continue;
        }
        if best.is_none_or(|(rank, _)| e.rank > rank) {
            best = Some((e.rank, enc));
        }
    }
    best.map(|(_, e)| intern(e))
}

impl Backends {
    /// Select against the catalogue and the live registry.
    pub fn probe(decode_pref: Accel, encode_pref: Accel) -> Result<Self> {
        let cat = catalogue::global();
        let req = Request {
            container: cat.programme_container.clone().or_else(|| Some("flv".into())),
            decode: decode_pref,
            encode: encode_pref,
            graphics: Accel::Auto,
            ..Request::default()
        };
        let sel = cat.select(&req, &GstRegistry)?;
        catalogue::log_selection(&sel);
        Ok(Self::from_selection(&sel))
    }

    /// The same shape, from a selection somebody else already made, so the
    /// mixer probes once and the source path reuses the answer.
    pub fn from_selection(sel: &Selection) -> Self {
        Self {
            video_decode: DecoderChoice {
                accel: accel_of(&sel.video_decode.accel),
                element: intern(&sel.video_decode.element),
                download: sel
                    .video_decode
                    .download
                    .as_deref()
                    .filter(|d| exists(d))
                    .map(intern),
            },
            video_encode: EncoderChoice {
                accel: accel_of(&sel.video_encode.accel),
                element: intern(&sel.video_encode.element),
            },
            audio_decode: intern(&sel.audio_decode.element),
            audio_encode: intern(&sel.audio_encode.element),
        }
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
    } else if t == bool::static_type() {
        el.set_property(prop, v != 0);
    } else if t.is_a(glib::Type::ENUM) || t.is_a(glib::Type::FLAGS) {
        // A catalogue that writes `preset = 10` against an enum property.
        set_enum(el, prop, &v.to_string());
    } else {
        warn!(element = %el.name(), prop, ?t, "unexpected property type, skipping");
    }
}

pub fn set_float(el: &gst::Element, prop: &str, v: f64) {
    let Some(pspec) = writable_property(el, prop) else {
        return;
    };
    let t = pspec.value_type();
    if t == f64::static_type() {
        el.set_property(prop, v);
    } else if t == f32::static_type() {
        el.set_property(prop, v as f32);
    } else {
        set_int(el, prop, v.round() as i64);
    }
}

pub fn set_bool(el: &gst::Element, prop: &str, v: bool) {
    let Some(pspec) = writable_property(el, prop) else {
        return;
    };
    // The same name can be a boolean on one backend and an enum on the next.
    // `zerolatency` is a gboolean on the old nvh264enc and the catalogue will
    // meet a build where it is not, so coerce rather than panic.
    let t = pspec.value_type();
    if t == bool::static_type() {
        el.set_property(prop, v);
    } else {
        set_int(el, prop, v as i64);
    }
}

/// Set a string property, leaving an element that has no such property alone.
///
/// Plugins outside the core's control rename properties between versions, and a
/// preview setting that has moved must degrade to the default rather than stop
/// the element being built.
pub fn set_str(el: &gst::Element, prop: &str, v: &str) {
    let Some(pspec) = writable_property(el, prop) else {
        return;
    };
    if pspec.value_type() == String::static_type() {
        el.set_property(prop, v);
    } else {
        el.set_property_from_str(prop, v);
    }
}

/// Every nickname an enum property on this element accepts, in the order the
/// enum declares them. Empty when there is no such property or it is not an
/// enum.
///
/// Read off the element rather than written down here, because the list
/// belongs to whichever version of the plugin is installed: `videotestsrc`
/// has gained patterns over the years, and a list typed into this repository
/// would refuse one that works on the machine in front of you.
pub fn enum_nicks(el: &gst::Element, prop: &str) -> Vec<String> {
    let Some(pspec) = el.find_property(prop) else {
        return Vec::new();
    };
    let t = pspec.value_type();
    if !t.is_a(glib::Type::ENUM) {
        return Vec::new();
    }
    match glib::EnumClass::with_type(t) {
        Some(class) => class.values().iter().map(|v| v.nick().to_string()).collect(),
        None => Vec::new(),
    }
}

/// The same list, for an element that has not been made yet. Used by a kind's
/// `validate`, which runs before anything is built and must not panic or cost
/// a pipeline.
pub fn factory_enum_nicks(factory: &str, prop: &str) -> Vec<String> {
    match gst::ElementFactory::make(factory).build() {
        Ok(el) => enum_nicks(&el, prop),
        Err(_) => Vec::new(),
    }
}

/// Set an enum property by nickname, refusing rather than panicking when the
/// element has never heard of it.
///
/// `set_property_from_str` panics on an unknown value, and a panic on the
/// mixer thread takes the programme with it. Anything that comes from an
/// operator, a config file or an API call goes through here.
pub fn try_set_enum(el: &gst::Element, prop: &str, nick: &str) -> Result<()> {
    let nicks = enum_nicks(el, prop);
    if nicks.is_empty() {
        anyhow::bail!(
            "{} has no enum property `{prop}` on this build of GStreamer",
            el.factory().map(|f| f.name().to_string()).unwrap_or_else(|| el.name().to_string())
        );
    }
    anyhow::ensure!(
        nicks.iter().any(|n| n == nick),
        "`{nick}` is not one of the {prop} values this build accepts. It takes: {}",
        nicks.join(", ")
    );
    el.set_property_from_str(prop, nick);
    Ok(())
}

/// Set an enum or flags property by its nickname. Nicknames are stable across
/// plugin versions in a way that numeric enum values are not.
///
/// The nickname is checked before it is used. `set_property_from_str` panics
/// on a value the enum does not know, and since the catalogue is data now, a
/// nickname a different version of the element has never heard of has to be a
/// warning: `preset = "p4"` is right for the current nvcodec and wrong for the
/// one on a five year old distribution, and neither should stop a broadcast.
pub fn set_enum(el: &gst::Element, prop: &str, nick: &str) {
    let Some(pspec) = writable_property(el, prop) else {
        return;
    };
    let t = pspec.value_type();
    if t.is_a(glib::Type::ENUM) {
        let class = glib::EnumClass::with_type(t);
        if class.as_ref().and_then(|c| c.value_by_nick(nick)).is_none()
            && class.as_ref().and_then(|c| c.value(nick.parse::<i32>().unwrap_or(i32::MIN))).is_none()
        {
            warn!(element = %el.name(), prop, nick, "this element version has no such value, skipping");
            return;
        }
    } else if t.is_a(glib::Type::FLAGS) {
        let class = glib::FlagsClass::with_type(t);
        if class.as_ref().and_then(|c| c.value_by_nick(nick)).is_none() {
            warn!(element = %el.name(), prop, nick, "this element version has no such flag, skipping");
            return;
        }
    } else if t != String::static_type() {
        // A number or a boolean written as a string in the catalogue.
        if let Ok(n) = nick.parse::<i64>() {
            set_int(el, prop, n);
            return;
        }
    }
    el.set_property_from_str(prop, nick);
}

/// Samples of encoder delay the AAC encoder does not take out of its
/// timestamps, so audio leaves it that much late relative to video.
///
/// Measured with `browser/dev/tail-offset.sh`: a file whose flash and beep
/// coincide comes out of the programme tail with the beep 43 ms late through
/// fdkaacenc and 21 ms late through avenc_aac, whatever the video encoder,
/// with or without the mixers in the path. Those are the two encoders'
/// documented priming delays (2048 and 1024 samples at 48 kHz). The numbers
/// now live in `codecs.toml` as `priming_delay_ms`, which is where somebody
/// measuring a third encoder can put theirs.
pub fn audio_encoder_delay_samples(element: &str) -> u64 {
    audio_encoder_delay_ms(element) * 48_000 / 1000
}

pub fn audio_encoder_delay_ms(element: &str) -> u64 {
    catalogue::global()
        .audio
        .iter()
        .find(|e| e.encoder.as_deref() == Some(element))
        .and_then(|e| e.priming_delay_ms)
        .unwrap_or(0)
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

    /// The software floor is the whole promise of the catalogue: a machine
    /// with no GPU still encodes.
    #[test]
    fn software_is_always_available_as_a_floor() {
        init();
        let b = Backends::probe(Accel::Software, Accel::Software)
            .expect("the software entries must resolve on every machine");
        assert_eq!(b.video_encode.accel, Accel::Software);
        assert_eq!(b.video_decode.accel, Accel::Software);
    }

    #[test]
    fn requesting_absent_hardware_fails_loudly() {
        init();
        // No machine has every backend, so at least one forced request must
        // fail. Assert on whichever is genuinely missing here.
        let cat = catalogue::global();
        for want in [Accel::Nvidia, Accel::Va, Accel::VideoToolbox, Accel::D3d11, Accel::Amf] {
            let name = want.name().unwrap();
            let installed = cat
                .video
                .iter()
                .any(|e| e.accel == name && e.decoder.as_deref().is_some_and(exists));
            if installed {
                continue;
            }
            let err = Backends::probe(want, Accel::Auto).expect_err("{want:?} should be absent");
            let text = format!("{err:#}");
            assert!(text.contains(name), "the error should name the accel: {text}");
            assert!(
                text.contains("rank"),
                "the error should list the entries that were available: {text}"
            );
        }
    }

    #[test]
    fn defensive_setters_ignore_unknown_properties() {
        init();
        let el = gst::ElementFactory::make("queue").build().unwrap();
        // Must not panic even though queue has none of these.
        set_int(&el, "bitrate", 4000);
        set_bool(&el, "realtime", true);
        set_enum(&el, "rc-mode", "cbr");
        set_float(&el, "quality", 0.5);
        // And a real property must still take effect, coerced to the right width.
        set_int(&el, "max-size-buffers", 42);
        assert_eq!(el.property::<u32>("max-size-buffers"), 42);
    }

    /// The catalogue is data, so a nickname from a different version of an
    /// element will turn up sooner or later. It must be a warning.
    #[test]
    fn an_enum_nickname_this_version_does_not_know_is_skipped() {
        init();
        let el = gst::ElementFactory::make("videotestsrc").build().unwrap();
        let before = el.property_value("pattern");
        set_enum(&el, "pattern", "not-a-pattern-anyone-has");
        assert_eq!(
            format!("{:?}", el.property_value("pattern")),
            format!("{before:?}"),
            "an unknown nickname must leave the property alone"
        );
        set_enum(&el, "pattern", "ball");
        assert_ne!(format!("{:?}", el.property_value("pattern")), format!("{before:?}"));
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

    #[test]
    fn the_priming_delay_comes_from_the_catalogue() {
        init();
        assert_eq!(audio_encoder_delay_ms("avenc_aac"), 21);
        assert_eq!(audio_encoder_delay_ms("fdkaacenc"), 43);
        assert_eq!(audio_encoder_delay_ms("something-nobody-measured"), 0);
    }

    /// Interning is what lets the rest of the code keep `&'static str` while
    /// the names come from a file. It must hand back one pointer per name.
    #[test]
    fn interning_a_name_twice_gives_the_same_pointer() {
        let a = intern("x264enc");
        let b = intern("x264enc");
        assert!(std::ptr::eq(a, b));
    }
}
