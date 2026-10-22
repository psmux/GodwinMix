use super::*;
use crate::Document;

const BTN36: &str = include_str!("../../tests/fixtures/btn36.conf");

fn live() -> Live {
    let video = MediaTrack { content: "video", track_id: "v1".into(), codec: "h264".into(), width: Some(1280), height: Some(720), fps: Some(29.97), bitrate: Some(1500), ..Default::default() };
    let audio = MediaTrack { content: "audio", track_id: "a1".into(), codec: "aac".into(), channels: Some(2), sample_rate: Some(48000), language: Some("eng".into()), ..Default::default() };
    let input = MediaTrack { pid: Some(0x31), codec: "mp2v".into(), ..video.clone() };
    Live { input_live: true, output_live: true, bitrate_kbps: 1650, input_kbps: 9000, lifetime_ms: 61_000, tracks_out: vec![video, audio], tracks_in: vec![input], ..Default::default() }
}

#[test]
fn a_running_stream_answers_every_field_the_wall_reads() {
    let doc = Document::parse(BTN36).unwrap();
    let s = doc.config.streams.iter().find(|s| s.name == "CH117").unwrap();
    let v = stream(s, &live());
    assert_eq!(v["name"], "CH117");
    assert_eq!(v["inputs"][0]["url"], s.inputs[0].url);
    let st = &v["stats"];
    assert_eq!((st["status"].as_str(), st["alive"].as_bool()), (Some("running"), Some(true)));
    assert_eq!((st["bitrate"].as_u64(), st["input_bitrate"].as_u64(), st["lifetime"].as_u64()), (Some(1650), Some(9000), Some(61_000)));
    assert_eq!(st["media_info"]["tracks"][0]["width"], 1280);
    assert_eq!(st["media_info"]["tracks"][1]["language"], "eng");
    assert_eq!(st["inputs"][0]["media_info"]["tracks"][0]["pid"], 0x31);
    assert_eq!(st["input"]["error_rate"], 0.0);
    assert_eq!(st["transcoder"]["restarts"], 0);
    let t = &v["transcoder"];
    assert_eq!((t["global"]["hw"].as_str(), t["global"]["gop"].as_u64()), (Some("nvenc"), Some(50)));
    let video = &t["tracks"][0];
    assert_eq!((video["content"].as_str(), video["bitrate"].as_u64()), (Some("video"), Some(1_500_000)));
    assert_eq!((video["size"]["width"].as_u64(), video["size"]["height"].as_u64()), (Some(1280), Some(720)));
    assert_eq!(t["tracks"][1]["bitrate"], 128_000);
}

#[test]
fn a_stream_that_is_not_flowing_is_not_running() {
    let doc = Document::parse(BTN36).unwrap();
    let s = &doc.config.streams[0];
    let mut l = live();
    l.output_live = false;
    l.scrambled_at = vec![1_792_756_804_000];
    let st = stats(s, &l);
    assert_eq!((st["status"].as_str(), st["bitrate"].as_u64()), (Some("starting"), Some(0)));
    assert_eq!(st["media_info"]["tracks"].as_array().map(Vec::len), Some(0));
    assert_eq!(st["inputs"][0]["stop_events"][0]["code"], "scrambled");
    let faithiq = doc.config.streams.iter().find(|s| s.name == "FAITHIQ").unwrap();
    assert!(stream(faithiq, &l).get("transcoder").is_none());
    assert_eq!(list(&[(s.clone(), l)])["streams"].as_array().map(Vec::len), Some(1));
}
