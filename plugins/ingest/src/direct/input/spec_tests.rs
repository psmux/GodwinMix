use super::*;

#[test]
fn every_scheme_names_its_kind() {
    for (uri, kind) in [
        ("udp://@239.1.1.1:5000", Kind::Udp),
        ("rtp://@239.1.1.1:5000", Kind::Udp),
        ("srt://10.0.0.1:9000", Kind::Srt),
        ("rtsp://cam/stream", Kind::Rtsp),
        ("https://cdn/live.m3u8", Kind::Http),
        ("https://cdn/live.mpd", Kind::Http),
        ("rtmp://host/app/key", Kind::Rtmp),
        ("rist://@0.0.0.0:5004", Kind::Rist),
        ("file:///clip.ts", Kind::File),
        ("/clips/clip.mp4", Kind::File),
        ("channel:church/main", Kind::Channel),
    ] {
        assert_eq!(InputSpec::new(uri).kind(), Ok(kind), "{uri}");
    }
}

#[test]
fn a_spec_reads_back_as_it_was_written() {
    let v = json!({"uri": "udp://@239.1.1.1:5000", "program": 2,
                   "params": {"interface": "en0"}, "backup": "srt://10.0.0.1:9000"});
    let spec = InputSpec::from_json(&v).unwrap();
    assert_eq!(spec.program, Some(2));
    assert_eq!(spec.param("interface").as_deref(), Some("en0"));
    assert_eq!(spec.backup.as_ref().unwrap().uri, "srt://10.0.0.1:9000");
    assert_eq!(InputSpec::from_json(&spec.json()).unwrap(), spec);
}

#[test]
fn a_bad_address_or_program_is_refused_with_the_way_out() {
    let err = InputSpec::from_json(&json!({"uri": "smb://share/clip.ts"})).unwrap_err();
    assert!(err.message.contains("udp://@239.1.1.1:5000"), "{err}");
    assert_eq!(err.data["field"], "uri");
    let err = InputSpec::from_json(&json!({"uri": "udp://@239.1.1.1:5000", "program": 0})).unwrap_err();
    assert_eq!(err.data["field"], "program");
}
