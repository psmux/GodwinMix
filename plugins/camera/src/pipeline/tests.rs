use super::*;
use serde_json::json;

fn canvas() -> Canvas {
    Canvas::new(1280, 720, 30)
}

#[test]
fn the_chain_ends_at_the_canvas_contract() {
    let chain = chain(&Settings::from(&json!({})), canvas());
    assert!(chain.contains("format=I420"), "{chain}");
    assert!(
        chain.contains("width=1280,height=720,framerate=30/1"),
        "{chain}"
    );
    assert!(chain.contains("videorate"), "{chain}");
    assert!(
        chain.starts_with(&format!("capsfilter name={HEAD}")),
        "{chain}"
    );
}

#[test]
fn a_test_pattern_builds_a_whole_pipeline_on_any_machine() {
    godwinmix_capture_common::init().unwrap();
    let settings = Settings::from(&json!({"element": "videotestsrc"}));
    let (pipeline, via) = build(&settings, canvas(), Transport::Container, "", Route::Fast)
        .expect("a test pattern builds everywhere");
    assert!(pipeline.by_name(device::SOURCE).is_some());
    assert!(pipeline.by_name("gmx-video-queue").is_some());
    assert_eq!(via, "videotestsrc");
}

#[test]
fn a_socket_transport_with_no_address_is_refused_before_anything_opens() {
    godwinmix_capture_common::init().unwrap();
    let settings = Settings::from(&json!({"element": "videotestsrc"}));
    let err =
        build(&settings, canvas(), Transport::Unixfd, "", Route::Fast).expect_err("no address");
    assert!(err.contains("container"), "{err}");
}

#[test]
fn the_rate_floor_is_the_best_the_device_has_at_that_size_up_to_the_canvas() {
    gst::init().unwrap();
    // The modes a UVC webcam lists through Kernel Streaming, in its order.
    let ks: gst::Caps = "video/x-raw,format=YUY2,width=1920,height=1080,framerate=5/1; \
                         image/jpeg,width=1920,height=1080,framerate=30/1; \
                         video/x-raw,format=YUY2,width=640,height=480,framerate=30/1"
        .parse()
        .unwrap();
    assert_eq!(rate_floor(&ks, (1920, 1080), 30), Some(30));
    assert_eq!(rate_floor(&ks, (1920, 1080), 25), Some(25));
    assert_eq!(rate_floor(&ks, (800, 600), 30), None);
    // A range, as a virtual camera lists it.
    let ranged: gst::Caps = "video/x-raw,width=1280,height=720,framerate=[1/1,60/1]"
        .parse()
        .unwrap();
    assert_eq!(rate_floor(&ranged, (1280, 720), 30), Some(30));
    // A camera that only manages 15 at that size is asked for 15, not 30.
    let slow: gst::Caps = "image/jpeg,width=1280,height=720,framerate=15/1"
        .parse()
        .unwrap();
    assert_eq!(rate_floor(&slow, (1280, 720), 30), Some(15));
}
