use super::*;
use crate::catalogue::select::FakeRegistry;

/// GStreamer's own ranks for the decoders these tests name, as the 1.28
/// runtimes register them.
fn stock(name: &str) -> Option<i32> {
    Some(match name {
        "avdec_h264" | "avdec_h265" | "avdec_aac" | "avdec_ac3" | "mpg123audiodec" => 256,
        "d3d11h264dec" => 257,
        "openh264dec" => 64,
        "faad" | "a52dec" | "mfaacdec" => 128,
        _ => return None,
    })
}

#[test]
fn libav_is_recognised_by_name() {
    assert!(is_libav("avdec_h264"));
    assert!(is_libav("avenc_aac"));
    assert!(!is_libav("openh264dec"));
    assert!(!is_libav("vah264dec"));
}

/// The case the change is for: a Linux box with openh264, faad, a52dec and
/// gst-libav. Each preferred decoder takes libav's place and libav drops one
/// under it, still pluggable.
#[test]
fn the_preferred_software_decoder_takes_libav_s_rank() {
    let cat = Catalogue::shipped().unwrap();
    let reg = FakeRegistry::with(&[
        "openh264enc", "openh264dec", "avdec_h264", "faad", "avdec_aac", "a52dec", "avdec_ac3",
        "mpg123audiodec",
    ]);
    let plan: BTreeMap<String, i32> = rank_plan(&cat, &reg, &stock).into_iter().collect();
    assert_eq!(plan.get("openh264dec"), Some(&256), "{plan:?}");
    assert_eq!(plan.get("avdec_h264"), Some(&255), "{plan:?}");
    assert_eq!(plan.get("faad"), Some(&256), "{plan:?}");
    assert_eq!(plan.get("avdec_aac"), Some(&255), "{plan:?}");
    assert_eq!(plan.get("a52dec"), Some(&256), "{plan:?}");
    assert_eq!(plan.get("mpg123audiodec"), None, "nothing to swap for MP3: {plan:?}");
}

/// A hardware decoder is not software and is never moved; it already beat
/// libav and still beats the decoder that took libav's place.
#[test]
fn hardware_decoders_keep_their_rank() {
    let cat = Catalogue::shipped().unwrap();
    let reg = FakeRegistry::with(&["d3d11h264dec", "openh264enc", "openh264dec", "avdec_h264"]);
    let plan = rank_plan(&cat, &reg, &stock);
    assert!(plan.iter().all(|(e, _)| e != "d3d11h264dec"), "{plan:?}");
    assert!(plan.iter().any(|(e, r)| e == "openh264dec" && *r < 257), "{plan:?}");
}

/// An operator who ranks libav first gets libav first, in decodebin too.
#[test]
fn a_catalogue_that_prefers_libav_moves_nothing() {
    let mut cat = Catalogue::shipped().unwrap();
    cat.video.iter_mut().filter(|e| e.id() == "h264-software-decode").for_each(|e| e.rank = 99);
    let reg = FakeRegistry::with(&["openh264enc", "openh264dec", "avdec_h264"]);
    assert!(rank_plan(&cat, &reg, &stock).is_empty());
}

/// Without gst-libav there is nothing to swap, and nothing taken from it.
#[test]
fn without_libav_nothing_is_taken_from_it() {
    let cat = Catalogue::shipped().unwrap();
    let reg = FakeRegistry::with(&["openh264enc", "openh264dec", "voaacenc", "faad", "x264enc"]);
    assert!(rank_plan(&cat, &reg, &stock).is_empty());
    assert!(in_use(&cat, &reg).is_empty(), "{:?}", in_use(&cat, &reg));
    let gone = undecodable(&cat, &reg);
    assert!(gone.contains(&"h265".to_string()), "{gone:?}");
    assert!(!gone.contains(&"h264".to_string()), "{gone:?}");
}

/// A machine with only libav for AAC says so, for `gmx doctor`.
#[test]
fn a_codec_only_libav_handles_is_named() {
    let cat = Catalogue::shipped().unwrap();
    let reg = FakeRegistry::with(&["openh264enc", "openh264dec", "avenc_aac", "avdec_aac"]);
    let used = in_use(&cat, &reg);
    assert!(used.contains(&("encode", "aac".into(), "avenc_aac".into())), "{used:?}");
    assert!(used.contains(&("decode", "aac".into(), "avdec_aac".into())), "{used:?}");
    assert!(!used.iter().any(|(_, c, _)| c == "h264"), "{used:?}");
}

fn base(extra: &[&str]) -> FakeRegistry {
    let mut names = vec!["compositor", "videoconvert", "h264parse", "aacparse", "flvmux"];
    names.extend_from_slice(extra);
    FakeRegistry::with(&names)
}

/// With gst-libav and the alternatives both installed, selection takes the
/// alternatives: openh264 to decode, VisualOn to encode AAC, faad to decode it.
#[test]
fn selection_prefers_what_is_not_libav() {
    use crate::catalogue::select::Request;
    let cat = Catalogue::shipped().unwrap();
    let linux = base(&["x264enc", "openh264enc", "openh264dec", "avdec_h264", "voaacenc", "avenc_aac", "faad", "avdec_aac"]);
    let sel = cat.select(&Request::default(), &linux).unwrap();
    assert_eq!(sel.video_encode.element, "x264enc");
    assert_eq!(sel.video_decode.element, "openh264dec");
    assert_eq!(sel.audio_encode.element, "voaacenc");
    assert_eq!(sel.audio_encode.priming_delay_ms, Some(33));
    assert_eq!(sel.audio_decode.element, "faad");

    let windows = base(&["openh264enc", "openh264dec", "avdec_h264", "voaacenc", "mfaacenc", "mfaacdec", "avenc_aac", "avdec_aac"]);
    let sel = cat.select(&Request::default(), &windows).unwrap();
    assert_eq!(sel.audio_encode.element, "mfaacenc");
    assert_eq!(sel.audio_encode.priming_delay_ms, Some(0));
    assert_eq!(sel.audio_decode.element, "mfaacdec");

    let mac = base(&["x264enc", "avdec_h264", "atenc", "atdec", "avenc_aac", "avdec_aac", "faad"]);
    let sel = cat.select(&Request::default(), &mac).unwrap();
    assert_eq!(sel.audio_encode.element, "atenc");
    assert_eq!(sel.audio_decode.element, "faad", "atdec is ranked under faad");
}

/// The whole programme selects with no gst-libav at all.
#[test]
fn a_machine_without_libav_selects_everything() {
    use crate::catalogue::select::Request;
    let cat = Catalogue::shipped().unwrap();
    let sel = cat.select(&Request::default(), &base(&["openh264enc", "openh264dec", "voaacenc", "faad"])).unwrap();
    for chosen in [&sel.video_decode, &sel.video_encode, &sel.audio_decode, &sel.audio_encode] {
        assert!(!is_libav(&chosen.element), "{} came from libav", chosen.element);
    }
}
