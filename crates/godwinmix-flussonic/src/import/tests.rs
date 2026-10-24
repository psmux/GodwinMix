use super::*;
use crate::Document;

const SAMPLE: &str = include_str!("../../tests/fixtures/sample-headend.conf");

fn of(text: &str) -> Import {
    import(&Document::parse(text).unwrap().config)
}

#[test]
fn a_headend_config_is_51_renditions_and_one_copy() {
    let i = of(SAMPLE);
    assert_eq!(i.shows.len(), 52);
    assert_eq!(i.shows.iter().filter(|s| s.copy).count(), 1);
    let ch = i.shows.iter().find(|s| s.name == "CH117").unwrap();
    assert_eq!(ch.input, json!({"uri": "udp://@239.10.0.1:2001", "program": 7, "params": {"interface": "eth1"}}));
    let r = &ch.outputs[0]["rendition"];
    assert_eq!(r["video"]["codec"], "h264");
    assert_eq!((r["video"]["width"].as_u64(), r["video"]["height"].as_u64()), (Some(1280), Some(720)));
    assert_eq!(r["video"]["bitrate_kbps"], 1500);
    assert_eq!((r["video"]["fit"].as_str(), r["video"]["pad_color"].as_str()), (Some("pad"), Some("#000000")));
    assert_eq!((r["video"]["keyframe_frames"].as_u64(), r["video"]["deinterlace"].as_str()), (Some(50), Some("off")));
    // Exactly the request the media side plans from; the planner takes NVENC first on its own.
    assert_eq!(
        *r,
        json!({"video": {"codec": "h264", "width": 1280, "height": 720, "bitrate_kbps": 1500, "keyframe_frames": 50,
                         "fit": "pad", "pad_color": "#000000", "deinterlace": "off", "preset": "p1"},
               "audio": {"codec": "aac", "bitrate_kbps": 128, "tracks": "all"}})
    );
    assert_eq!((r["audio"]["codec"].as_str(), r["audio"]["bitrate_kbps"].as_u64()), (Some("aac"), Some(128)));
    assert_eq!(r["audio"]["tracks"], "all");
    // Flussonic's paths serve TS without a key, so every output makes TS beside CMAF, open.
    assert_eq!((ch.outputs[0]["params"]["segments"].as_str(), ch.outputs[0]["params"]["access"].as_str()), (Some("both"), Some("open")));
    assert_eq!(ch.outputs[0]["params"]["segment_ms"], 3400);
    let relay = i.shows.iter().find(|s| s.name == "RELAY1").unwrap();
    assert!(relay.copy && relay.outputs[0].get("rendition").is_none());
    assert_eq!(relay.outputs[0]["uri"], "hls://flussonic");
    // The DVR is not carried. Every field of the line is acted on, so the
    // sample's 51 transcoders, black pads and no deinterlacing, say nothing.
    assert!(i.notes.iter().any(|(_, n)| n.what.starts_with("dvr archive")));
    assert_eq!(i.notes.len(), 1, "{:?}", i.notes);
}

#[test]
fn unknown_directives_and_options_are_reported_with_their_lines() {
    let text = "http 8090;\ncluster_key abc;\nstream A {\n  input udp://239.0.0.1:1234;\n  backup_strategy x;\n  transcoder vb=1m size=640x360 ab=64k weird=1;\n}\n";
    let i = of(text);
    let lines: Vec<(usize, &str)> = i.notes.iter().map(|(_, n)| (n.line, n.what.as_str())).collect();
    assert!(lines.contains(&(2, "directive 'cluster_key'")), "{lines:?}");
    assert!(lines.contains(&(5, "directive 'backup_strategy'")), "{lines:?}");
    assert!(lines.iter().any(|(l, w)| *l == 3 && w.contains("weird=1")), "{lines:?}");
    let r = &i.shows[0].outputs[0]["rendition"];
    assert_eq!((r["video"]["width"].as_u64(), r["video"]["fit"].as_str()), (Some(640), Some("stretch")));
}

#[test]
fn a_second_input_is_the_backup_and_pushes_are_outputs() {
    let text = "stream A {\n  title The A channel;\n  disabled true;\n  segment_duration 4;\n  segment_count 5;\n  input rtmp://h/app/a;\n  input udp://239.0.0.1:1;\n  push srt://10.0.0.9:9000;\n}\n";
    let s = &of(text).shows[0];
    assert_eq!(s.input["backup"], json!({"uri": "udp://@239.0.0.1:1"}));
    assert_eq!(s.outputs[0]["params"], json!({"segments": "both", "access": "open", "segment_ms": 4000, "window": 20}));
    assert_eq!(s.outputs[1], json!({"id": "push-1", "uri": "srt://10.0.0.9:9000"}));
    assert!(s.disabled && s.copy);
    assert_eq!(s.title.as_deref(), Some("The A channel"));
}

#[test]
fn copy_bitrates_mean_no_rendition_for_that_part() {
    let s = &of("stream A {\n  input rtmp://h/a;\n  transcoder vb=copy ab=96k;\n}\n").shows[0];
    let r = &s.outputs[0]["rendition"];
    assert!(r.get("video").is_none() && r["audio"]["bitrate_kbps"] == 96, "{r}");
    let s = &of("stream A {\n  input rtmp://h/a;\n  transcoder vb=copy ab=copy;\n}\n").shows[0];
    assert!(s.copy);
}

#[test]
fn a_title_change_is_not_a_media_change() {
    let a = &of("stream A {\n  input rtmp://h/a;\n}\n").shows[0];
    let b = &of("stream A {\n  title New;\n  input rtmp://h/a;\n}\n").shows[0];
    let c = &of("stream A {\n  input rtmp://h/b;\n}\n").shows[0];
    assert!(a.same_media(b) && !a.same_media(c));
}
