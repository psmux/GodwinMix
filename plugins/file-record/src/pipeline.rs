//! The recording pipeline.
//!
//! ```text
//!  appsrc <- a thread reading the programme FIFO ! parsebin ! splitmuxsink
//! ```
//!
//! Three elements and no decoder. The programme arrives already encoded and
//! muxed in streamable Matroska, which is what an output receives, so
//! recording it is a remux: `parsebin` takes the container apart into parsed
//! elementary streams and `splitmuxsink` puts them into a file. A 1080p60
//! recording costs about as much CPU as copying a file, whatever the canvas
//! is.
//!
//! The front of it is an `appsrc` fed by a thread rather than the `fdsrc` you
//! would expect. `fdsrc` waits on the descriptor with `poll`, and `poll` on a
//! FIFO does not report readable on macOS however much is written into it:
//! measured here, the reader sat in the poll for twenty seven seconds while
//! the writer blocked on a full pipe. `capture_common::fifo::Pump` reads the
//! descriptor on a thread instead, which is the same shape the core already
//! uses to read a pipe on Windows.
//!
//! `parsebin` produces its pads when it has seen the stream, so they are
//! linked as they appear rather than in the description.

use gstreamer as gst;
use gstreamer::prelude::*;

use godwinmix_capture_common::{capture, elements};
use godwinmix_sdk::plugin::Reporter;

use crate::settings::Settings;

/// The muxer and its properties, per format.
///
/// MP4 is written in fragments. A normal MP4 keeps the index in memory and
/// writes it when the file is closed, so a machine that loses power leaves a
/// file no player will open, which is exactly the recording you wanted most. A
/// fragmented one writes an index every second, and the worst a crash costs is
/// the last fragment.
/// `async-finalize` is deliberately left off. It closes each finished file on
/// its own thread so a split does not stall the pipeline, and the price is
/// that the last file is still being written when the pipeline stops. For a
/// recording, a file that is definitely finished is worth more than a split
/// that never hiccups, and fragmented MP4 makes the finalise cheap anyway.
fn muxer(settings: &Settings) -> (&'static str, Option<&'static str>) {
    if settings.extension() == "mkv" {
        ("matroskamux", None)
    } else {
        ("mp4mux", Some("properties,fragment-duration=1000"))
    }
}

/// The element the FIFO pump pushes the programme into.
pub const INPUT: &str = "gmx-in";

/// Build the recorder. The programme arrives through [`INPUT`].
pub fn build(
    settings: &Settings,
    location: &str,
    reporter: Option<Reporter>,
) -> Result<gst::Pipeline, String> {
    let (factory, properties) = muxer(settings);
    if !elements::exists(factory) {
        return Err(format!(
            "this machine has no GStreamer '{factory}' element. Install \
             gstreamer1.0-plugins-good, or set `format` to the other one."
        ));
    }
    let mut description = format!(
        "appsrc name={INPUT} format=bytes is-live=false block=true max-bytes=8388608 \
         ! parsebin name=gmx-parse \
         splitmuxsink name=gmx-record muxer-factory={factory}"
    );
    if let Some(properties) = properties {
        description.push_str(&format!(" muxer-properties=\"{properties}\""));
    }
    let pipeline = capture::build(&description)?;
    let sink = pipeline
        .by_name("gmx-record")
        .ok_or_else(|| "the pipeline lost its recorder".to_string())?;
    sink.set_property("location", location);
    if let Some(split) = settings.split_after() {
        elements::set_number(&sink, "max-size-time", split.as_nanos() as i64);
    } else {
        // Zero is "never by time". The size limit is left alone at its default
        // for the same reason: an operator who asked for one file means one.
        elements::set_number(&sink, "max-size-time", 0);
    }
    link_parsed_streams(&pipeline, &sink, reporter)?;
    Ok(pipeline)
}

/// Link each stream `parsebin` finds to the right request pad on the recorder.
///
/// This has to happen while the pipeline is running, because `parsebin` only
/// knows what is in the programme once it has seen some of it. A pad that
/// nobody links answers `not-linked` upstream and the whole pipeline stops
/// with "internal data stream error", so every step here is checked and
/// anything that goes wrong is reported rather than swallowed.
fn link_parsed_streams(
    pipeline: &gst::Pipeline,
    sink: &gst::Element,
    reporter: Option<Reporter>,
) -> Result<(), String> {
    let parse = pipeline
        .by_name("gmx-parse")
        .ok_or_else(|| "the pipeline lost its parser".to_string())?;
    let sink = sink.clone();
    parse.connect_pad_added(move |_, pad| {
        let Some((template, what)) = stream_kind(pad) else {
            report(
                &reporter,
                "the programme carries a stream that is neither picture nor sound, and a file \
                 cannot hold it"
                    .to_string(),
            );
            return;
        };
        let Some(request) = sink.request_pad_simple(template) else {
            report(
                &reporter,
                format!("the recorder would not give a pad for the {what}"),
            );
            return;
        };
        // A queue per stream, between the parser and the recorder.
        //
        // Without it the two deadlock on a programme that has both picture and
        // sound: the recorder holds the picture back while it collects a whole
        // group of frames, the sound piles up behind the parser with nowhere
        // to go, and the parser stops reading. Measured here as a file that
        // was created and stayed at nought bytes. The queue is generous
        // because the thing it is absorbing is one group of frames, which at
        // a two second interval is two seconds of programme.
        let Some(queue) = buffer_for(pad, &request, &reporter) else {
            return;
        };
        report(&reporter, format!("recording the {what}"));
        let _ = queue;
    });
    Ok(())
}

/// Put a queue between a parsed stream and the recorder, and link all three.
fn buffer_for(
    pad: &gst::Pad,
    request: &gst::Pad,
    reporter: &Option<Reporter>,
) -> Option<gst::Element> {
    let pipeline = request.parent_element().and_then(|e| e.parent())?;
    let pipeline = pipeline.downcast::<gst::Pipeline>().ok()?;
    let queue = gst::ElementFactory::make("queue")
        .property("max-size-buffers", 0u32)
        .property("max-size-bytes", 0u32)
        .property("max-size-time", 10 * gst::ClockTime::SECOND.nseconds())
        .build()
        .ok()?;
    if pipeline.add(&queue).is_err() {
        report(
            reporter,
            "could not add a buffer for the stream".to_string(),
        );
        return None;
    }
    let _ = queue.sync_state_with_parent();
    let (sink_pad, src_pad) = (queue.static_pad("sink")?, queue.static_pad("src")?);
    if let Err(e) = pad.link(&sink_pad) {
        report(reporter, format!("could not reach the buffer: {e}"));
        return None;
    }
    if let Err(e) = src_pad.link(request) {
        report(reporter, format!("could not reach the recorder: {e}"));
        return None;
    }
    Some(queue)
}

/// Which request pad this stream wants, and what to call it in a log line.
///
/// A pad `parsebin` has just added usually has no caps yet: they arrive with
/// the first buffer, which is after this. The `GstStream` on the pad is
/// already there, because `parsebin` builds it from the stream start event, so
/// that is what decides. Caps are the fallback for a source that somehow has
/// them first.
fn stream_kind(pad: &gst::Pad) -> Option<(&'static str, &'static str)> {
    if let Some(stream) = pad.stream() {
        let kind = stream.stream_type();
        if kind.contains(gst::StreamType::VIDEO) {
            return Some(("video", "picture"));
        }
        if kind.contains(gst::StreamType::AUDIO) {
            return Some(("audio_%u", "sound"));
        }
    }
    let caps = pad.current_caps()?;
    let name = caps.structure(0)?.name();
    if name.starts_with("video/") {
        Some(("video", "picture"))
    } else if name.starts_with("audio/") {
        Some(("audio_%u", "sound"))
    } else {
        None
    }
}

fn report(reporter: &Option<Reporter>, message: String) {
    if let Some(r) = reporter {
        r.info(message);
    }
}

/// How many bytes are in the file, or files, this recorder has written.
pub fn bytes_on_disk(folder: &std::path::Path, since: std::time::SystemTime) -> u64 {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file() && m.modified().is_ok_and(|t| t >= since))
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mp4_is_fragmented_and_matroska_is_not() {
        let (factory, properties) = muxer(&Settings::from(&json!({"format": "mp4"})));
        assert_eq!(factory, "mp4mux");
        assert!(properties.unwrap().contains("fragment-duration"));
        let (factory, properties) = muxer(&Settings::from(&json!({"format": "mkv"})));
        assert_eq!(factory, "matroskamux");
        assert!(properties.is_none());
    }

    #[test]
    fn a_recorder_builds_and_carries_the_location_and_the_split() {
        godwinmix_capture_common::init().unwrap();
        // The programme comes in through an appsrc, not an fdsrc. See the
        // module comment for the macOS poll that made that necessary.
        let settings = Settings::from(&json!({"split_after_minutes": 30}));
        let pipeline = build(&settings, "/tmp/gmx-test-%05d.mp4", None).expect("it builds");
        let sink = pipeline
            .by_name("gmx-record")
            .expect("the recorder is named");
        assert_eq!(
            sink.property::<Option<String>>("location").as_deref(),
            Some("/tmp/gmx-test-%05d.mp4")
        );
        assert_eq!(
            sink.property::<u64>("max-size-time"),
            30 * 60 * 1_000_000_000
        );
    }

    #[test]
    fn one_file_means_no_time_limit() {
        godwinmix_capture_common::init().unwrap();
        let pipeline = build(&Settings::default(), "/tmp/gmx-test.mp4", None).expect("it builds");
        let sink = pipeline
            .by_name("gmx-record")
            .expect("the recorder is named");
        assert_eq!(sink.property::<u64>("max-size-time"), 0);
    }

    #[test]
    fn counting_bytes_in_a_folder_that_is_not_there_is_zero_rather_than_an_error() {
        assert_eq!(
            bytes_on_disk(
                std::path::Path::new("/no/such/folder"),
                std::time::UNIX_EPOCH
            ),
            0
        );
    }

    #[test]
    fn counting_bytes_sees_a_file_written_after_the_mark() {
        let dir = std::env::temp_dir().join(format!("gmx-bytes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let before = std::time::SystemTime::now() - std::time::Duration::from_secs(10);
        std::fs::write(dir.join("a.mp4"), vec![0u8; 2048]).unwrap();
        assert_eq!(bytes_on_disk(&dir, before), 2048);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
