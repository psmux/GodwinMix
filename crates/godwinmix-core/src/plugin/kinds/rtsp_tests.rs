use super::*;

fn params(s: &str) -> Params {
    toml::from_str(s).unwrap()
}

#[test]
fn every_rtsp_scheme_is_a_live_stream_and_nothing_else_is() {
    for uri in ["rtsp://h/s", "RTSPT://h/s", "rtspu://h/s", "rtsph://h/s", "rtsps://h/s", "rtspst://h/s"] {
        assert!(is_rtsp(uri), "{uri}");
        let got = crate::plugin::source::resolve(uri).expect("claimed");
        assert_eq!(got.manifest.provide_id(), "hls/source", "{uri} must not fall through to a clip");
    }
    for uri in ["rtmp://h/a/k", "srt://h:9000", "https://h/live.m3u8", "/clips/rtsp.mp4"] {
        assert!(!is_rtsp(uri), "{uri}");
    }
}

#[test]
fn a_plain_address_tries_udp_then_tcp_unless_told() {
    let t = Tuning::of("rtsp://cam/1", &Params::new());
    assert_eq!(t, Tuning { latency_ms: LATENCY_MS, protocols: Some("udp-mcast+udp+tcp") });
    assert_eq!(Tuning::of("rtsp://cam/1", &params("transport = \"tcp\"")).protocols, Some("tcp"));
    assert_eq!(Tuning::of("rtsps://cam/1", &params("transport = \"UDP\"")).protocols, Some("udp"));
}

#[test]
fn a_scheme_that_names_its_transport_wins_over_the_param() {
    for uri in ["rtspt://cam/1", "rtspu://cam/1", "rtsph://cam/1"] {
        assert_eq!(Tuning::of(uri, &params("transport = \"udp\"")).protocols, None, "{uri}");
    }
}

#[test]
fn latency_is_taken_and_kept_in_range() {
    assert_eq!(Tuning::of("rtsp://c", &params("latency_ms = 1500")).latency_ms, 1500);
    validate(&params("latency_ms = 0")).unwrap();
    let refused = validate(&params("latency_ms = 20000")).unwrap_err().to_string();
    assert!(refused.contains("0 to 10000"), "{refused}");
    assert!(validate(&params("latency_ms = \"fast\"")).is_err());
}

#[test]
fn an_unknown_transport_is_refused_with_the_choices() {
    validate(&params("transport = \"auto\"")).unwrap();
    let refused = validate(&params("transport = \"quic\"")).unwrap_err().to_string();
    assert!(refused.contains("\"auto\", \"tcp\" or \"udp\""), "{refused}");
}

/// The numbers reach a real `rtspsrc`, when this install has one.
#[test]
fn the_numbers_reach_rtspsrc() {
    let _ = gst::init();
    let Ok(src) = gst::ElementFactory::make("rtspsrc").build() else {
        println!("skipping: no rtspsrc in this GStreamer");
        return;
    };
    Tuning::of("rtsp://cam/1", &params("transport = \"tcp\"\nlatency_ms = 300")).apply(&src);
    assert_eq!(src.property::<u32>("latency"), 300);
    assert_eq!(src.property::<u64>("tcp-timeout"), SILENCE_US);
    assert_eq!(src.property::<u64>("timeout"), SILENCE_US);
    assert_eq!(src.property::<u64>("teardown-timeout"), TEARDOWN_US);
    assert!(src.property::<bool>("do-rtsp-keep-alive"));
    let protocols = format!("{:?}", src.property_value("protocols"));
    assert!(protocols.to_lowercase().contains("tcp") && !protocols.to_lowercase().contains("udp"), "{protocols}");
}
