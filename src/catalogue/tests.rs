//! Two kinds of test here.
//!
//! The first needs no hardware and no GStreamer at all: the shipped catalogue
//! is parsed, validated and run through selection against a registry made of
//! strings. That is what a GPU free CI runner exercises on every commit, and
//! it is how a machine with an NVIDIA card can still prove that the fallback
//! path is the one a laptop would take.
//!
//! The second runs against this machine's real registry, because the promise
//! that matters is that the software entry resolves everywhere.

use super::apply::Vars;
use super::model::{PropValue, Role};
use super::select::{FakeRegistry, GstRegistry, Request};
use super::*;
use crate::config::{Accel, Config};

/// A machine with no GPU: the reference `laptop`.
fn laptop() -> FakeRegistry {
    FakeRegistry::with(&[
        "compositor",
        "videoconvert",
        "x264enc",
        "avdec_h264",
        "h264parse",
        "avenc_aac",
        "avdec_aac",
        "aacparse",
        "flvmux",
    ])
}

/// The same machine without gst-plugins-ugly, so openh264 is the floor.
fn permissive_only() -> FakeRegistry {
    FakeRegistry::with(&[
        "compositor",
        "videoconvert",
        "openh264enc",
        "openh264dec",
        "h264parse",
        "avenc_aac",
        "avdec_aac",
        "aacparse",
        "flvmux",
    ])
}

fn with(base: FakeRegistry, extra: &[&str]) -> FakeRegistry {
    let mut set = base.0;
    set.extend(extra.iter().map(|s| s.to_string()));
    FakeRegistry(set)
}

// ---------------------------------------------------------------------------
// The shipped catalogue itself
// ---------------------------------------------------------------------------

#[test]
fn the_shipped_catalogue_parses() {
    let cat = Catalogue::shipped().expect("codecs.toml must parse");
    assert!(cat.video.len() > 10, "the vendor table is the point of the file");
    assert!(!cat.audio.is_empty());
    assert!(!cat.graphics.is_empty());
    assert!(!cat.container.is_empty());
}

/// The whole CI safe validation in one place: ids unique, every entry
/// licensed, every element name plausible, every derived property carrying a
/// unit that can actually convert.
#[test]
fn the_shipped_catalogue_is_well_formed() {
    let cat = Catalogue::shipped().unwrap();
    let problems = cat.validate();
    assert!(problems.is_empty(), "codecs.toml has problems:\n  {}", problems.join("\n  "));
}

#[test]
fn every_entry_carries_a_license() {
    let cat = Catalogue::shipped().unwrap();
    for e in &cat.video {
        assert!(e.license.is_some(), "video entry {} has no license", e.id());
    }
    for e in &cat.audio {
        assert!(e.license.is_some(), "audio entry {} has no license", e.id());
    }
    for e in &cat.graphics {
        assert!(e.license.is_some(), "graphics entry {} has no license", e.id());
    }
}

/// A bitrate written without a unit would send a 4,000 bit/s programme or a
/// 4 Gbit/s one, and both fail in confusing ways. Anything derived must say
/// what unit the element wants it in.
#[test]
fn every_derived_property_names_a_unit_and_a_variable_that_exist() {
    let cat = Catalogue::shipped().unwrap();
    let mut derived = 0;
    let mut check = |id: &str, props: &std::collections::BTreeMap<String, PropValue>| {
        for (name, v) in props {
            if let PropValue::Derived(d) = v {
                derived += 1;
                assert!(
                    super::apply::check_derived(d).is_none(),
                    "{id}: property {name}: {}",
                    super::apply::check_derived(d).unwrap()
                );
            }
        }
    };
    for e in &cat.video {
        check(&e.id(), &e.properties);
    }
    for e in &cat.audio {
        check(&e.id(), &e.properties);
    }
    assert!(derived > 10, "the unit indirection should be used widely, found {derived}");
}

#[test]
fn the_software_entries_are_always_there() {
    let cat = Catalogue::shipped().unwrap();
    for id in ["h264-software-openh264", "h264-software-x264", "aac-avenc", "software"] {
        assert!(
            cat.video_entry(id).is_some()
                || cat.audio_entry(id).is_some()
                || cat.graphics_entry(id).is_some(),
            "{id} is the fallback and must be in the catalogue"
        );
    }
}

/// The board honesty commitment from 09: the catalogue carries the Pi 4's
/// encoder and says in plain words that the Pi 5 has none.
#[test]
fn the_pi_entry_says_what_the_pi_5_does_not_have() {
    let cat = Catalogue::shipped().unwrap();
    let pi = cat.video_entry("h264-v4l2").expect("the V4L2 entry is the Pi 4's");
    let comment = pi.comment.clone().unwrap_or_default();
    assert!(comment.contains("Pi 5"), "the Pi 5 has no encoder and the entry must say so");
}

// ---------------------------------------------------------------------------
// Selection against a registry made of strings
// ---------------------------------------------------------------------------

#[test]
fn a_machine_with_no_gpu_gets_the_software_entry() {
    let cat = Catalogue::shipped().unwrap();
    let sel = cat.select(&Request::default(), &laptop()).unwrap();
    assert_eq!(sel.video_encode.element, "x264enc");
    assert_eq!(sel.video_encode.accel, "software");
    assert_eq!(sel.video_decode.element, "avdec_h264");
    assert_eq!(sel.audio_encode.element, "avenc_aac");
    assert_eq!(sel.graphics.compositor, "compositor");
    assert_eq!(sel.graphics.convert, "videoconvert");
}

/// Without gst-plugins-ugly there is no x264. openh264 is BSD, ships
/// everywhere, and is what is left.
#[test]
fn without_x264_the_permissive_entry_carries_the_programme() {
    let cat = Catalogue::shipped().unwrap();
    let sel = cat.select(&Request::default(), &permissive_only()).unwrap();
    assert_eq!(sel.video_encode.element, "openh264enc");
    assert_eq!(sel.video_encode.license, "BSD-2-Clause");
}

#[test]
fn hardware_wins_on_rank_when_it_is_installed() {
    let cat = Catalogue::shipped().unwrap();
    let gpu = with(laptop(), &["nvh264enc", "nvh264dec"]);
    let sel = cat.select(&Request::default(), &gpu).unwrap();
    assert_eq!(sel.video_encode.element, "nvh264enc");
    assert_eq!(sel.video_decode.element, "nvh264dec");
    assert_eq!(sel.video_encode.rank, 240);
}

/// The configuration the catalogue exists to handle: NVDEC present, NVENC
/// unusable. The entry names both elements and `requires` names the encoder,
/// and the decoder must survive that.
#[test]
fn nvdec_without_nvenc_keeps_the_fast_decoder_and_the_software_encoder() {
    let cat = Catalogue::shipped().unwrap();
    let box_ = with(laptop(), &["nvh264dec"]);
    let sel = cat.select(&Request::default(), &box_).unwrap();
    assert_eq!(sel.video_decode.element, "nvh264dec", "the decoder is installed and usable");
    assert_eq!(sel.video_encode.element, "x264enc", "and the encoder falls back on its own");
}

#[test]
fn pinning_a_backend_that_is_not_here_fails_with_the_list_of_ones_that_are() {
    let cat = Catalogue::shipped().unwrap();
    let req = Request { encode: Accel::Nvidia, ..Default::default() };
    let err = cat.select(&req, &laptop()).expect_err("nvidia is not installed on a laptop");
    let text = format!("{err:#}");
    assert!(text.contains("nvidia"), "{text}");
    assert!(text.contains("h264-software-x264"), "the error must list what was available: {text}");
    assert!(text.contains("installed"), "{text}");
}

#[test]
fn pinning_software_is_always_satisfiable() {
    let cat = Catalogue::shipped().unwrap();
    let gpu = with(laptop(), &["nvh264enc", "nvh264dec"]);
    let req = Request { encode: Accel::Software, decode: Accel::Software, ..Default::default() };
    let sel = cat.select(&req, &gpu).unwrap();
    assert_eq!(sel.video_encode.accel, "software");
    assert_eq!(sel.video_decode.accel, "software");
}

/// The container decides what codecs are eligible. RTMP carries H.264 and AAC
/// and nothing else, so an AV1 entry at a higher rank must not be picked for
/// an FLV programme.
#[test]
fn the_container_keeps_selection_to_codecs_it_can_carry() {
    let mut cat = Catalogue::shipped().unwrap();
    cat.set_rank("av1-software", 900);
    let av1 = with(laptop(), &["svtav1enc", "dav1ddec", "av1parse", "mpegtsmux"]);
    let flv = cat.select(&Request::default(), &av1).unwrap();
    assert_eq!(flv.video_encode.codec, "h264", "flv cannot carry AV1");

    let ts = Request { container: Some("mpegts".into()), ..Default::default() };
    let sel = cat.select(&ts, &av1).unwrap();
    assert_eq!(sel.video_encode.codec, "av1");
    assert_eq!(sel.video_encode.element, "svtav1enc");
}

// ---------------------------------------------------------------------------
// Graphics
// ---------------------------------------------------------------------------

/// Every GPU compositor is rank none upstream with open bugs on dynamic pad
/// add and remove, which is exactly what a mixer does. So the software entry
/// stays the default until somebody writes a `verified` record for the
/// platform, and pinning is how that record gets earned.
#[test]
fn a_gpu_compositor_is_not_taken_until_somebody_has_verified_it_here() {
    let cat = Catalogue::shipped().unwrap();
    let gpu = with(
        laptop(),
        &["nvh264enc", "nvh264dec", "cudacompositor", "cudaconvert", "cudaupload", "cudadownload"],
    );
    let sel = cat.select(&Request::default(), &gpu).unwrap();
    assert_eq!(sel.graphics.id, "software", "unverified GPU entries are not taken on their own");
    assert!(sel.graphics.why.contains("verified"), "{}", sel.graphics.why);

    let pinned = Request { graphics: Accel::Cuda, ..Default::default() };
    let sel = cat.select(&pinned, &gpu).unwrap();
    assert_eq!(sel.graphics.compositor, "cudacompositor");
    assert_eq!(sel.graphics.memory, "cuda");
    assert!(sel.graphics.is_gpu());
}

#[test]
fn a_verified_gpu_entry_is_taken_on_its_own() {
    let mut cat = Catalogue::shipped().unwrap();
    let here = super::select::current_platform();
    let gl = cat.graphics.iter_mut().find(|g| g.id() == "gl").unwrap();
    gl.verified.push(model::Verified { platform: here, ..Default::default() });
    let reg = with(laptop(), &["glvideomixer", "glcolorconvert", "gldownload", "glupload"]);
    let sel = cat.select(&Request::default(), &reg).unwrap();
    assert_eq!(sel.graphics.id, "gl");
    assert_eq!(sel.graphics.memory, "gl");
}

// ---------------------------------------------------------------------------
// Merging: config, file, environment
// ---------------------------------------------------------------------------

/// The acceptance line from the roadmap: an operator who adds an AV1 entry to
/// `[codecs]` on a machine that has the elements gets an AV1 programme, with
/// no rebuild.
#[test]
fn an_operator_can_add_an_av1_entry_and_get_an_av1_programme() {
    let text = r#"
[codecs]
programme_container = "mpegts"

[[codecs.video]]
id = "av1-house"
codec = "av1"
accel = "software"
rank = 900
encoder = "svtav1enc"
decoder = "dav1ddec"
parser = "av1parse"
license = "BSD-3-Clause"
container = ["mpegts"]
properties = { preset = 10, "target-bitrate" = { unit = "kbit", from = "video.bitrate_kbps" } }
"#;
    let cfg: Config = toml::from_str(text).expect("the [codecs] table must parse");
    let mut cat = Catalogue::shipped().unwrap();
    cat.overlay(cfg.codecs.clone());
    assert!(cat.validate().is_empty(), "{:?}", cat.validate());

    let reg = with(laptop(), &["svtav1enc", "dav1ddec", "av1parse", "mpegtsmux"]);
    let req = super::request_from(&cfg, &cat);
    assert_eq!(req.container.as_deref(), Some("mpegts"));
    let sel = cat.select(&req, &reg).unwrap();
    assert_eq!(sel.video_encode.id, "av1-house");
    assert_eq!(sel.video_encode.element, "svtav1enc");
    assert_eq!(sel.video_encode.parser.as_deref(), Some("av1parse"));
}

/// An entry whose id matches a shipped one replaces it outright, so an
/// operator fixing a renamed property does not inherit half of the old entry.
#[test]
fn an_override_replaces_the_shipped_entry_of_the_same_id() {
    let text = r#"
[[codecs.video]]
id = "h264-software-x264"
codec = "h264"
accel = "software"
rank = 1
encoder = "x264enc"
decoder = "avdec_h264"
parser = "h264parse"
license = "GPL-2.0-or-later"
container = ["flv"]
properties = { "speed-preset" = "ultrafast" }
"#;
    let cfg: Config = toml::from_str(text).unwrap();
    let mut cat = Catalogue::shipped().unwrap();
    let before = cat.video.len();
    cat.overlay(cfg.codecs.clone());
    assert_eq!(cat.video.len(), before, "an override must not add a second entry");
    let e = cat.video_entry("h264-software-x264").unwrap();
    assert_eq!(e.rank, 1);
    assert_eq!(e.properties.len(), 1, "the entry is replaced, not merged");

    // And at rank 1 it loses to openh264, which is the point of being able to
    // demote one from a config file.
    let reg = with(permissive_only(), &["x264enc", "avdec_h264"]);
    let sel = cat.select(&Request::default(), &reg).unwrap();
    assert_eq!(sel.video_encode.element, "openh264enc");
}

#[test]
fn the_environment_can_nudge_a_rank_without_restating_an_entry() {
    let mut cat = Catalogue::shipped().unwrap();
    cat.apply_env_from(|k| match k {
        "GMX_CODEC_RANK" => Some("h264-software-x264=10".into()),
        _ => None,
    });
    assert_eq!(cat.video_entry("h264-software-x264").unwrap().rank, 10);
    let sel = cat
        .select(&Request::default(), &with(permissive_only(), &["x264enc", "avdec_h264"]))
        .unwrap();
    assert_eq!(sel.video_encode.element, "openh264enc");
}

#[test]
fn the_environment_can_take_an_entry_out() {
    let mut cat = Catalogue::shipped().unwrap();
    cat.apply_env_from(|k| match k {
        "GMX_CODEC_DISABLE" => Some("h264-software-x264".into()),
        _ => None,
    });
    assert!(cat.video_entry("h264-software-x264").unwrap().disabled);
    let sel = cat
        .select(&Request::default(), &with(permissive_only(), &["x264enc", "avdec_h264"]))
        .unwrap();
    assert_eq!(sel.video_encode.element, "openh264enc");
}

// ---------------------------------------------------------------------------
// Containers, for the outputs
// ---------------------------------------------------------------------------

#[test]
fn the_container_lookup_is_what_an_output_muxes_with() {
    let flv = container_for("flv").expect("rtmp needs flv");
    assert_eq!(flv.muxer, "flvmux");
    assert!(flv.carries_video("h264"));
    assert!(flv.carries_audio("aac"));
    assert!(!flv.carries_video("av1"), "flv has never carried AV1");
    assert!(flv.streamable);
    assert!(container_for("no-such-container").is_none());
}

// ---------------------------------------------------------------------------
// The needs rule, which is subtle enough to deserve its own test
// ---------------------------------------------------------------------------

#[test]
fn requires_naming_the_other_roles_element_does_not_block_this_role() {
    let cat = Catalogue::shipped().unwrap();
    let nv = cat.video_entry("h264-nvidia").unwrap();
    assert_eq!(nv.requires, vec!["nvh264enc".to_string()]);
    assert!(nv.needs(Role::Encode).contains(&"nvh264enc".to_string()));
    assert!(
        !nv.needs(Role::Decode).contains(&"nvh264enc".to_string()),
        "the decoder must not need the encoder to exist"
    );
    assert!(nv.needs(Role::Decode).contains(&"nvh264dec".to_string()));
}

// ---------------------------------------------------------------------------
// Against this machine's real registry
// ---------------------------------------------------------------------------

fn gst() {
    let _ = gstreamer::init();
}

#[test]
fn selection_resolves_on_this_machine() {
    gst();
    let cat = Catalogue::shipped().unwrap();
    let sel = cat.select(&Request::default(), &GstRegistry).expect("every machine must resolve");
    assert!(crate::probe::exists(&sel.video_encode.element));
    assert!(crate::probe::exists(&sel.video_decode.element));
    assert!(crate::probe::exists(&sel.audio_encode.element));
    assert!(crate::probe::exists(&sel.audio_decode.element));
    assert!(crate::probe::exists(&sel.graphics.compositor));
}

#[test]
fn the_software_entry_resolves_on_this_machine_when_it_is_forced() {
    gst();
    let cat = Catalogue::shipped().unwrap();
    let req = Request {
        encode: Accel::Software,
        decode: Accel::Software,
        graphics: Accel::Software,
        ..Default::default()
    };
    let sel = cat.select(&req, &GstRegistry).expect("the software floor must hold everywhere");
    assert_eq!(sel.video_encode.accel, "software");
    assert_eq!(sel.graphics.compositor, "compositor");
    assert_eq!(sel.graphics.convert, "videoconvert");
    assert!(crate::probe::exists(&sel.video_encode.element));
}

/// `codec.list` is a plain function returning serialisable data. The API layer
/// has nothing to do but hand it to serde.
#[test]
fn codec_list_serialises() {
    gst();
    let listing = list();
    assert!(!listing.entries.is_empty());
    assert!(listing.entries.iter().any(|e| e.present), "something must be installed here");
    let json = serde_json::to_string(&listing).expect("codec.list has to serialise");
    assert!(json.contains("\"platform\""));
    assert!(json.contains("\"considered\""));
}

/// Properties from the catalogue must actually land on a real element. This
/// is the unit indirection end to end.
#[test]
fn catalogue_properties_reach_a_real_encoder() {
    gst();
    let cat = Catalogue::shipped().unwrap();
    let Some(e) = cat.video_entry("h264-software-x264") else { return };
    if !crate::probe::exists("x264enc") {
        return;
    }
    let el = gstreamer::ElementFactory::make("x264enc").build().unwrap();
    let vars = Vars { video_bitrate_kbps: 2500, keyframe_frames: 60, fps: 30, ..Default::default() };
    super::apply::apply(&el, &e.properties, &vars);
    super::apply::apply_keyframe(&el, e.keyframe.as_ref(), &vars);
    use gstreamer::prelude::*;
    assert_eq!(el.property::<u32>("bitrate"), 2500, "kbit stays kbit for x264enc");
    assert_eq!(el.property::<u32>("key-int-max"), 60);
    assert!(!el.property::<bool>("byte-stream"), "flv wants AVC sample format");
}

/// openh264 wants bits where x264 wants kilobits, and that conversion is the
/// reason units are in the file at all.
#[test]
fn the_same_catalogue_bitrate_reaches_openh264_in_bits() {
    gst();
    if !crate::probe::exists("openh264enc") {
        return;
    }
    let cat = Catalogue::shipped().unwrap();
    let e = cat.video_entry("h264-software-openh264").unwrap();
    let el = gstreamer::ElementFactory::make("openh264enc").build().unwrap();
    let vars = Vars { video_bitrate_kbps: 2500, ..Default::default() };
    super::apply::apply(&el, &e.properties, &vars);
    use gstreamer::prelude::*;
    assert_eq!(el.property::<u32>("bitrate"), 2_500_000);
}

/// `gmx doctor` calls this. Every entry gets a line, present or not, and a
/// present one is backed by a real one second encode.
#[test]
fn doctor_lines_cover_every_entry() {
    gst();
    let cat = Catalogue::shipped().unwrap();
    // The software decode only entries and the absent ones are cheap; the
    // present encoders each cost a second. Run against a fake registry that
    // has nothing, so this test stays fast and still proves the shape.
    let lines = check::doctor_lines(&cat, &FakeRegistry::with(&[]));
    assert_eq!(lines.len(), cat.video.len() + cat.audio.len() + cat.graphics.len());
    assert!(lines.iter().all(|l| l.starts_with("codec ") || l.starts_with("graphics ")));
    assert!(lines.iter().any(|l| l.contains("not installed here")));
}

/// And one real one, on whatever this machine's software entry is, so the one
/// second encode path is exercised rather than only described.
#[test]
fn the_software_entry_survives_a_one_second_encode_here() {
    gst();
    let cat = Catalogue::shipped().unwrap();
    let req = Request { encode: Accel::Software, ..Default::default() };
    let sel = cat.select(&req, &GstRegistry).unwrap();
    let report = check::test_entry(&cat, &sel.video_encode.id, 1.0, 320, 180, 30)
        .expect("the software entry must round trip");
    assert!(report.ok, "{}", report.note);
    assert!(report.frames_out > 20, "got {} frames back", report.frames_out);
    let psnr = report.psnr_db.expect("a video round trip has a psnr");
    assert!(psnr > check::PSNR_FLOOR, "psnr {psnr} is too low to be the same picture");
}

// ---------------------------------------------------------------------------
// The other platforms
//
// The registry differs on every one of them, and the promise is that the
// software entry resolves regardless. These are the registries the reference
// machines in 09 actually have, written out as names so the check runs on any
// runner, including this Mac.
// ---------------------------------------------------------------------------

fn windows_registry() -> FakeRegistry {
    FakeRegistry::with(&[
        "compositor",
        "videoconvert",
        "openh264enc",
        "openh264dec",
        "avdec_h264",
        "h264parse",
        "avenc_aac",
        "avdec_aac",
        "aacparse",
        "flvmux",
        "mfh264enc",
        "d3d11h264dec",
        "d3d11download",
        "d3d11compositor",
        "d3d11convert",
        "d3d12h264dec",
        "d3d12download",
    ])
}

fn linux_intel_registry() -> FakeRegistry {
    FakeRegistry::with(&[
        "compositor",
        "videoconvert",
        "x264enc",
        "avdec_h264",
        "h264parse",
        "avenc_aac",
        "avdec_aac",
        "aacparse",
        "flvmux",
        "vah264enc",
        "vah264dec",
        "vapostproc",
        "vacompositor",
    ])
}

fn macos_registry() -> FakeRegistry {
    FakeRegistry::with(&[
        "compositor",
        "videoconvert",
        "x264enc",
        "avdec_h264",
        "h264parse",
        "avenc_aac",
        "avdec_aac",
        "aacparse",
        "flvmux",
        "vtenc_h264_hw",
        "vtdec_hw",
        "glvideomixer",
        "glcolorconvert",
        "gldownload",
        "glupload",
    ])
}

/// The Raspberry Pi 4: hardware encode through V4L2, software everything else.
fn pi4_registry() -> FakeRegistry {
    with(permissive_only(), &["v4l2h264enc", "v4l2h264dec"])
}

/// The Raspberry Pi 5, which has no hardware video encoder at all. This is the
/// entry in 09 that says the newer board is the harder target, written as a
/// test so nobody quietly assumes otherwise.
fn pi5_registry() -> FakeRegistry {
    permissive_only()
}

#[test]
fn every_platform_resolves_an_encoder_and_a_compositor() {
    let cat = Catalogue::shipped().unwrap();
    let machines: [(&str, FakeRegistry, &str); 6] = [
        ("windows", windows_registry(), "mfh264enc"),
        ("linux-intel", linux_intel_registry(), "vah264enc"),
        ("macos", macos_registry(), "vtenc_h264_hw"),
        ("pi4", pi4_registry(), "v4l2h264enc"),
        ("pi5", pi5_registry(), "openh264enc"),
        ("laptop", laptop(), "x264enc"),
    ];
    for (name, reg, expect) in machines {
        let sel = cat.select(&Request::default(), &reg).unwrap_or_else(|e| {
            panic!("{name} must resolve a programme encoder: {e:#}");
        });
        assert_eq!(sel.video_encode.element, expect, "on {name}");
        assert_eq!(sel.graphics.compositor, "compositor", "software stays the default on {name}");
        assert!(!sel.audio_encode.element.is_empty(), "on {name}");
    }
}

/// And whatever the machine has, forcing software must still work, because
/// that is the switch an operator reaches for when the GPU misbehaves during
/// a show.
#[test]
fn forcing_software_works_on_every_platform() {
    let cat = Catalogue::shipped().unwrap();
    let req = Request { encode: Accel::Software, decode: Accel::Software, ..Default::default() };
    for (name, reg) in [
        ("windows", windows_registry()),
        ("linux-intel", linux_intel_registry()),
        ("macos", macos_registry()),
        ("pi4", pi4_registry()),
        ("pi5", pi5_registry()),
    ] {
        let sel = cat
            .select(&req, &reg)
            .unwrap_or_else(|e| panic!("the software floor must hold on {name}: {e:#}"));
        assert_eq!(sel.video_encode.accel, "software", "on {name}");
        assert_eq!(sel.video_decode.accel, "software", "on {name}");
    }
}

/// The Pi 4 has an encoder and the Pi 5 does not, which is the whole reason
/// the comment is in the file.
#[test]
fn the_pi_4_encodes_in_hardware_and_the_pi_5_does_not() {
    let cat = Catalogue::shipped().unwrap();
    let pi4 = cat.select(&Request::default(), &pi4_registry()).unwrap();
    assert_eq!(pi4.video_encode.accel, "v4l2");
    let pi5 = cat.select(&Request::default(), &pi5_registry()).unwrap();
    assert_eq!(pi5.video_encode.accel, "software");
    assert_eq!(pi5.video_encode.element, "openh264enc");
}
