//! HEVC through a channel: a publisher sending enhanced RTMP HEVC (the tags
//! an SRT or RTMP HEVC encoder becomes on the hub), converted to H.264 for a
//! destination that only takes H.264, and H.264 converted to HEVC for one that
//! wants it. Real encoders and decoders, and the size read back out of what
//! comes out.

use super::*;

/// A 640x360 HEVC and AAC publisher on `hub`, framed as enhanced RTMP.
fn publish_hevc(hub: &Hub) -> gst::Pipeline {
    gmx_netkit::init().unwrap();
    let p = hub.publish("church", "main", "127.0.0.1:1", None).unwrap();
    let to = tagger::share(Box::new(Timed(p, Arc::new(AtomicU64::new(0)))));
    let zero = Arc::new(tagger::Zero::default());
    let pipeline = gst::parse::launch(
        "videotestsrc is-live=true pattern=ball ! video/x-raw,format=I420,width=640,height=360,framerate=30/1 \
         ! x265enc tune=zerolatency speed-preset=ultrafast key-int-max=30 bitrate=800 ! h265parse name=vp \
         audiotestsrc is-live=true ! audio/x-raw,rate=48000,channels=2 ! avenc_aac ! aacparse name=ap",
    )
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    for (parser, sink) in [("vp", tagger::hevc_sink(to.clone(), zero.clone())), ("ap", tagger::audio_sink(to, zero))] {
        pipeline.add(&sink).unwrap();
        pipeline.by_name(parser).unwrap().link(&sink).unwrap();
    }
    pipeline.set_state(gst::State::Playing).unwrap();
    pipeline
}

fn nodes(decode: Value, encode: Value) -> Vec<StreamSpec> {
    let scale = json!({"id": "scale:main:320x180p30", "kind": "scale", "input": "decode:main:video", "width": 320, "height": 180, "fps": [30, 1]});
    specs(&json!({"channels": [{"id": "church", "app": "church", "transcode": [{"stream": "main", "nodes": [decode, scale, encode]}]}]}))
}

fn available(elements: &[&str]) -> bool {
    gmx_netkit::init().unwrap();
    let missing: Vec<&&str> = elements.iter().filter(|e| gst::ElementFactory::find(e).is_none()).collect();
    if !missing.is_empty() {
        eprintln!("skipping: needs {missing:?}");
    }
    missing.is_empty()
}

/// Convert with `decode` and `encode`, and read what one destination gets.
fn convert(hub: &Hub, decode: Value, encode: Value, id: &str) -> Vec<MediaTag> {
    let t = Transcoders::new(hub.clone());
    t.apply(nodes(decode, encode), &[wanted("a", id, "rtmp://x/y/z")]);
    let key = output_key("main", Some(id), Some("copy:main:audio"));
    let renditions = t.renditions();
    wait_for("the converted pair", 20, || renditions.is_live("church", &key));
    // Three seconds, times GODWINMIX_TIMING_SLACK on a runner that says it
    // is slow: an HEVC decode and encode beside the rest of the suite on the
    // Windows runner sent fewer than 45 pictures in three.
    let slack = std::env::var("GODWINMIX_TIMING_SLACK").ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0).max(1.0);
    read_for(&renditions, &key, (3.0 * slack).ceil() as u64)
}

fn pictures(tags: &[MediaTag]) -> usize {
    tags.iter().filter(|t| t.kind == TagKind::Video).count()
}

#[test]
fn an_hevc_publisher_is_decoded_and_sent_on_as_h264() {
    if !available(&["x265enc", "avdec_h265", "x264enc"]) {
        return;
    }
    let hub = Hub::new();
    let source = publish_hevc(&hub);
    let header = read_for(&hub, "main", 2).into_iter().find(|t| t.kind == TagKind::Video && t.sequence_header).expect("a sequence start");
    let v = codec::read_video(&header);
    assert_eq!((v.codec.as_str(), v.width, v.height), ("h265", 640, 360), "the hub reads an HEVC publisher's size");
    let decode = json!({"id": "decode:main:video", "kind": "decode", "track": "video", "codec": "h265", "element": "avdec_h265", "parser": "h265parse"});
    let tags = convert(&hub, decode, encode(ENCODE, "scale:main:320x180p30", 320, 180, 300), ENCODE);
    assert_eq!(size(&tags), Some((320, 180)), "{:?}", first_tags(&tags));
    assert!(pictures(&tags) > 45, "{} pictures came through", pictures(&tags));
    drop(source);
}

#[test]
fn an_h264_publisher_is_sent_on_as_enhanced_rtmp_hevc() {
    if !available(&["x265enc", "x264enc", "avdec_h264"]) {
        return;
    }
    let hub = Hub::new();
    let (source, _) = publish(&hub);
    let id = "encode:main:h265:320x180p30:300k:g1000";
    let decode = json!({"id": "decode:main:video", "kind": "decode", "track": "video", "codec": "h264", "element": "avdec_h264", "parser": "h264parse"});
    let enc = json!({"id": id, "kind": "encode", "input": "scale:main:320x180p30", "codec": "h265", "element": "x265enc", "parser": "h265parse",
                     "width": 320, "height": 180, "fps": [30, 1], "bitrate_kbps": 300,
                     "props": {"tune": "zerolatency", "speed-preset": "ultrafast", "bitrate": 300, "key-int-max": 30}});
    let tags = convert(&hub, decode, enc, id);
    let header = tags.iter().find(|t| t.kind == TagKind::Video && t.sequence_header).expect("an HEVC sequence start");
    assert_eq!(crate::eflv::fourcc(&header.payload), Some(*crate::eflv::HEVC));
    let v = codec::read_video(header);
    assert_eq!((v.codec.as_str(), v.width, v.height), ("h265", 320, 180));
    assert!(pictures(&tags) > 45, "{} pictures came through", pictures(&tags));
    drop(source);
}
