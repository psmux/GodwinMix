//! Real mux, file and decoder tests for the portable output seam.
use godwinmix_core::config::OutputConfig;
use godwinmix_core::plugin::output::{self, OutputCtx};
use gstreamer as gst;
use gstreamer::prelude::*;

#[test]
fn recording_files_decode_and_restarts_preserve_existing_footage() {
    gst::init().unwrap();
    let folder = std::env::temp_dir().join(format!("gmx-record-test-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("existing.mp4"), b"keep me").unwrap();
    for format in ["mp4", "mkv"] {
        let mut cfg = OutputConfig::bare("archive", "record://programme");
        cfg.params.insert(
            "directory".into(),
            folder.to_string_lossy().to_string().into(),
        );
        cfg.params.insert("format".into(), format.into());
        let (mut output, _) = output::open(&cfg).unwrap();
        let mut paths = Vec::new();
        for generation in 0..2 {
            let pipeline = gst::parse::launch(
                "videotestsrc num-buffers=60 ! video/x-raw,width=320,height=180,framerate=30/1 \
                 ! x264enc tune=zerolatency ! h264parse ! queue name=video \
                 audiotestsrc num-buffers=94 samplesperbuffer=1024 \
                 ! audio/x-raw,rate=48000 ! audioconvert ! avenc_aac ! aacparse ! queue name=audio",
            )
            .unwrap()
            .downcast::<gst::Pipeline>()
            .unwrap();
            let params = cfg.effective_params();
            output
                .build(
                    &OutputCtx {
                        id: &cfg.id,
                        generation,
                        pipeline: &pipeline,
                        params: &params,
                        cfg: &cfg,
                    },
                    &pipeline.by_name("video").unwrap(),
                    &pipeline.by_name("audio").unwrap(),
                )
                .unwrap();
            pipeline.set_state(gst::State::Playing).unwrap();
            let message = pipeline
                .bus()
                .unwrap()
                .timed_pop_filtered(
                    gst::ClockTime::from_seconds(20),
                    &[gst::MessageType::Eos, gst::MessageType::Error],
                )
                .expect("recorder completes");
            if let gst::MessageView::Error(error) = message.view() {
                panic!("{} {:?}", error.error(), error.debug());
            }
            assert!(output.connected());
            let status = output.status();
            let path = status["recording_path"].as_str().unwrap().to_string();
            assert!(status["bytes_muxed"].as_u64().unwrap() > 0);
            pipeline.set_state(gst::State::Null).unwrap();
            decode(&path);
            paths.push(path);
        }
        assert_ne!(paths[0], paths[1]);
        assert!(std::path::Path::new(&paths[0]).exists());
    }
    assert_eq!(
        std::fs::read(folder.join("existing.mp4")).unwrap(),
        b"keep me"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

fn decode(path: &str) {
    let uri = glib::filename_to_uri(path, None).unwrap();
    let play = gst::ElementFactory::make("playbin")
        .property("uri", uri.as_str())
        .build()
        .unwrap();
    for property in ["video-sink", "audio-sink"] {
        let sink = gst::ElementFactory::make("fakesink")
            .property("sync", false)
            .build()
            .unwrap();
        play.set_property(property, sink);
    }
    play.set_state(gst::State::Playing).unwrap();
    let message = play
        .bus()
        .unwrap()
        .timed_pop_filtered(
            gst::ClockTime::from_seconds(15),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        )
        .expect("recorded file decodes to its end");
    play.set_state(gst::State::Null).unwrap();
    if let gst::MessageView::Error(error) = message.view() {
        panic!("{} {:?}", error.error(), error.debug());
    }
}

#[test]
fn invalid_recording_format_has_a_next_step() {
    let mut cfg = OutputConfig::bare("archive", "record://programme");
    cfg.params.insert("format".into(), "avi".into());
    let error = output::open(&cfg)
        .err()
        .expect("unsupported format is refused");
    assert!(error.to_string().contains("choose one"));
}
