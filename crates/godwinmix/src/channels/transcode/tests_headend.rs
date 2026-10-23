//! A headend's forty six channels planned at once: MPEG-2 in, H.264 720p
//! with bars and every sound track as AAC out, on a machine with an NVIDIA
//! card. Every one must land on NVDEC and NVENC with its pictures kept on
//! the GPU; a single x264 among them would be a fault.

use godwinmix_core::catalogue::select::FakeRegistry;
use godwinmix_core::catalogue::Catalogue;
use godwinmix_govern::{Governor, GovernorConfig, Profile};
use godwinmix_protocol::destination::StoredDestination;
use godwinmix_protocol::rendition::RenditionChoice;
use serde_json::{json, Value};

use super::{Machine, Transcode};
use crate::channels::{Live, Record};

fn nvidia() -> Machine {
    let names = ["nvh264enc", "nvh264dec", "nvmpeg2videodec", "cudaconvertscale", "cudadownload", "h264parse", "x264enc", "avdec_h264",
        "avdec_mpeg2video", "a52dec", "ac3parse", "avenc_aac", "avdec_aac", "aacparse"];
    Machine::probe(&Catalogue::shipped().unwrap(), &FakeRegistry::with(&names))
}

fn record(id: &str) -> Record {
    toml::from_str(&format!("id = \"{id}\"\nname = \"{id}\"\napp = \"{id}\"\n")).unwrap()
}

/// The request an imported transcoder line makes.
fn channel(id: &str) -> StoredDestination {
    let ask = json!({"video": {"codec": "h264", "width": 1280, "height": 720, "bitrate_kbps": 1500, "keyframe_frames": 50,
                               "fit": "pad", "pad_color": "#000000", "deinterlace": "off", "preset": "p1"},
                     "audio": {"codec": "aac", "bitrate_kbps": 128, "tracks": "all"}});
    StoredDestination {
        id: format!("{id}-hls"),
        platform: "hls".into(),
        label: id.into(),
        server: String::new(),
        key: None,
        stream: "*".into(),
        enabled: true,
        rendition: Some(serde_json::from_value::<RenditionChoice>(ask).unwrap()),
    }
}

/// A broadcast programme: 1080i, 720p60 or 480i MPEG-2, with one to three
/// AC-3 tracks, the first sometimes 5.1.
fn programme(id: &str, n: usize) -> Live {
    // As the line up it was measured on runs: eleven 1080i, six 720p60,
    // the rest 480i.
    let (w, h, fps) = match n {
        _ if n % 4 == 0 && n < 44 => (1920, 1080, 29.97),
        _ if n % 8 == 1 && n < 48 => (1280, 720, 59.94),
        _ => (704, 480, 29.97),
    };
    let tracks: Vec<Value> = (1..=(n % 3)).map(|t| json!({"track": t, "kind": "audio", "codec": "ac3", "channels": 2, "sample_rate": 48000, "kbps": 192})).collect();
    let mut l = Live::from_plugin(&json!({
        "channel": id, "app": id, "stream": "main", "since_ms": 1,
        "video": {"codec": "mpeg2", "width": w, "height": h, "fps": fps, "kbps": 8000, "frame_rate": fps, "interlaced": h != 720},
        "audio": {"codec": "ac3", "channels": if n % 2 == 0 { 6 } else { 2 }, "sample_rate": 48000, "kbps": 384},
        "tracks": tracks,
    }))
    .unwrap();
    l.channel = id.into();
    l
}

#[test]
fn forty_six_channels_all_go_to_nvdec_and_nvenc_and_stay_on_the_gpu() {
    let governor = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 48, 64 * 1024);
    let t = Transcode::with(nvidia(), governor);
    let ids: Vec<String> = (0..46).map(|n| format!("ch{n}")).collect();
    let records: Vec<Record> = ids.iter().map(|id| record(id)).collect();
    let lives: Vec<Live> = ids.iter().enumerate().map(|(n, id)| programme(id, n)).collect();
    t.replan(&records, &lives, |r| vec![channel(&r.id)]);
    for (n, id) in ids.iter().enumerate() {
        let streams = t.streams(id).unwrap_or_else(|| panic!("{id} has no plan: {:?}", t.view(id, &format!("{id}-hls"))));
        let nodes = streams[0]["nodes"].as_array().unwrap();
        let of = |kind: &'static str| nodes.iter().filter(move |v| v["kind"] == kind);
        assert!(of("encode").all(|e| e["element"] == "nvh264enc" && e["memory"] == "cuda" && e["keyframe_frames"] == 50 && e["profile"] == "main"), "{id}: {:?}", t.view(id, &format!("{id}-hls")).0.and_then(|p| p.encoder_reason));
        assert!(of("decode").filter(|d| d["track"] == "video").all(|d| d["element"] == "nvmpeg2videodec" && d["memory"] == "cuda"), "{id}");
        assert!(of("scale").all(|s| s["element"] == "cudaconvertscale" && s["rate"] == false), "{id}");
        assert_eq!(of("aencode").count(), 1 + n % 3, "{id}: one AAC encode per track");
        let row = t.row(id, &channel(id), json!({})).unwrap();
        assert_eq!(row["audio_tracks"].as_array().map_or(0, Vec::len), n % 3, "{id}: {row}");
    }
}

/// The encoder's own keyframe property for one programme at `fps`, planned
/// on `machine`: (element, value).
fn keyframe_prop(machine: Machine, fps: f64) -> (String, Value) {
    let governor = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 48, 64 * 1024);
    let t = Transcode::with(machine, governor);
    let mut l = Live::from_plugin(&json!({
        "channel": "ch", "app": "ch", "stream": "main", "since_ms": 1,
        "video": {"codec": "mpeg2", "width": 1280, "height": 720, "fps": fps, "kbps": 8000, "frame_rate": fps},
        "audio": {"codec": "ac3", "channels": 2, "sample_rate": 48000, "kbps": 192},
    }))
    .unwrap();
    l.channel = "ch".into();
    t.replan(&[record("ch")], &[l], |r| vec![channel(&r.id)]);
    let nodes = t.streams("ch").unwrap()[0]["nodes"].as_array().unwrap().clone();
    let e = nodes.iter().find(|n| n["kind"] == "encode").unwrap();
    let element = e["element"].as_str().unwrap().to_string();
    let prop = if element == "nvh264enc" { "gop-size" } else { "key-int-max" };
    (element, e["props"][prop].clone())
}

#[test]
fn a_gop_of_50_is_a_keyframe_every_50_frames_on_nvenc_and_x264_at_29_97_and_59_94() {
    let x264 = || Machine::probe(&Catalogue::shipped().unwrap(), &FakeRegistry::with(&["x264enc", "h264parse", "avdec_mpeg2video", "a52dec", "ac3parse", "avenc_aac", "aacparse"]));
    for fps in [29.97, 59.94] {
        assert_eq!(keyframe_prop(nvidia(), fps), ("nvh264enc".to_string(), json!(50)), "NVENC at {fps}");
        assert_eq!(keyframe_prop(x264(), fps), ("x264enc".to_string(), json!(50)), "x264 at {fps}");
    }
}
