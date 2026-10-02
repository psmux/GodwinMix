//! A registry directory longer than a Unix socket address can be, with real
//! `gmxbussink` and `gmxbussrc` elements: the picture still gets through.
//!
//! A `GODWINMIX_HOME` of more than about 80 bytes used to break every shared
//! camera and every channel source, because the socket path under it would
//! not bind and the reader failed with "Could not read from resource".

#![cfg(all(unix, feature = "gst"))]

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

#[test]
fn a_bus_under_a_home_path_over_110_bytes_carries_pictures() {
    gst::init().unwrap();
    godwinmix_framebus::gst::register().unwrap();
    let root = std::env::temp_dir().join(format!("fb-long-home-{}", std::process::id()));
    let dir = root
        .join("Users/someone/Library/Application Support/A Studio With A Long Name")
        .join("godwinmix-home-for-the-sunday-service")
        .join("bus");
    assert!(dir.as_os_str().len() > 110, "{} bytes", dir.as_os_str().len());
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.to_str().unwrap();
    let name = "channel:browser/chrome-macos";

    let owner = gst::parse::launch(&format!(
        "videotestsrc is-live=true ! video/x-raw,format=NV12,width=320,height=180,framerate=30/1 ! \
         gmxbussink bus-name={name} bus-dir=\"{dir}\""
    ))
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    owner.set_state(gst::State::Playing).unwrap();
    let reader = gst::parse::launch(&format!(
        "gmxbussrc bus-name={name} bus-dir=\"{dir}\" ! appsink name=out sync=false max-buffers=2 drop=true"
    ))
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let sink = reader.by_name("out").unwrap().downcast::<gst_app::AppSink>().unwrap();
    reader.set_state(gst::State::Playing).unwrap();

    let frames = (0..10)
        .filter(|_| sink.try_pull_sample(gst::ClockTime::from_seconds(5)).is_some())
        .count();
    reader.set_state(gst::State::Null).unwrap();
    owner.set_state(gst::State::Null).unwrap();
    // The socket is in the registry itself, where the listing finds it.
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(frames, 10, "the reader got {frames} of 10 frames");
}
