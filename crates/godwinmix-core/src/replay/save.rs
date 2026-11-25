//! Writing a clip to an MP4 without encoding it again.
//!
//! The frames go straight from the ring into a muxer, so a minute of a
//! 1080p angle is written in the time it takes the disk to take 45 MB. The
//! file starts at the keyframe at or before the in point (on a source angle,
//! at most a second early) and its timestamps start at zero.
//!
//! Blocking from start to finish. The caller runs it on a worker thread; it
//! never touches the mixer's pipelines, only the buffers the clip shares.

use super::clip::Clip;
use super::ring::Frame;
use crate::gstutil::make;
use anyhow::{bail, Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::path::Path;
use std::time::Duration;

/// What was written.
#[derive(Debug, Clone, Copy)]
pub struct Written {
    pub duration_ns: u64,
    pub bytes: u64,
}

/// The parser that puts a codec into the shape `mp4mux` takes.
fn parser_for(caps: &gst::Caps) -> Option<&'static str> {
    let name = caps.structure(0)?.name().as_str();
    Some(match name {
        "video/x-h264" => "h264parse",
        "video/x-h265" => "h265parse",
        "video/x-av1" => "av1parse",
        "video/x-vp9" => "vp9parse",
        "audio/mpeg" => "aacparse",
        "audio/x-opus" => "opusparse",
        _ => return None,
    })
}

fn input(pipeline: &gst::Pipeline, caps: &gst::Caps, mux: &gst::Element, tag: &str) -> Result<gst_app::AppSrc> {
    let src = gst_app::AppSrc::builder().name(format!("save-{tag}")).caps(caps).format(gst::Format::Time).max_bytes(0).build();
    let mut chain: Vec<gst::Element> = vec![src.clone().upcast()];
    if let Some(p) = parser_for(caps).filter(|p| crate::probe::exists(p)) {
        chain.push(make(p, &format!("save-{tag}-parse"))?);
    }
    pipeline.add_many(&chain)?;
    gst::Element::link_many(&chain)?;
    chain.last().expect("an appsrc at least").link(mux).with_context(|| format!("mp4mux will not take {caps}"))?;
    Ok(src)
}

/// A frame's buffer with its times counted from `zero`.
fn rebased(f: &Frame, zero: u64) -> gst::Buffer {
    let mut b = f.buffer.copy();
    let buf = b.make_mut();
    buf.set_pts(gst::ClockTime::from_nseconds(f.at.saturating_sub(zero)));
    buf.set_dts(f.dts.map(|d| gst::ClockTime::from_nseconds(d.saturating_sub(zero))));
    b
}

/// Write `clip` to `path`, finished and closed, and say how long it runs.
pub fn write(clip: &Clip, path: &Path) -> Result<Written> {
    let Some(vcaps) = clip.video_caps().cloned() else { bail!("the clip holds no picture, so there is nothing to save") };
    let zero = clip.video[0].frame.at;
    let pipeline = gst::Pipeline::with_name("replay-save");
    let mux = make("mp4mux", "save-mux")?;
    let sink = make("filesink", "save-file")?;
    sink.set_property("location", path.to_string_lossy().to_string());
    pipeline.add_many([&mux, &sink])?;
    mux.link(&sink)?;
    let video = input(&pipeline, &vcaps, &mux, "v")?;
    let audio = match &clip.audio_caps {
        Some(caps) if !clip.audio.is_empty() => Some(input(&pipeline, caps, &mux, "a")?),
        _ => None,
    };
    pipeline.set_state(gst::State::Playing).context("starting the replay writer")?;
    let result = feed(clip, zero, &video, audio.as_ref()).and_then(|()| wait(&pipeline, clip.length()));
    let _ = pipeline.set_state(gst::State::Null);
    result?;
    let end = clip.video.iter().map(|f| f.frame.end()).max().unwrap_or(zero);
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(Written { duration_ns: end.saturating_sub(zero), bytes })
}

/// Picture and sound in time order, so the muxer never waits on one of them.
fn feed(clip: &Clip, zero: u64, video: &gst_app::AppSrc, audio: Option<&gst_app::AppSrc>) -> Result<()> {
    let mut sound = clip.audio.iter().filter(|f| f.at >= zero).peekable();
    for cf in &clip.video {
        while let (Some(a), Some(src)) = (sound.peek(), audio) {
            if a.at > cf.frame.at {
                break;
            }
            src.push_buffer(rebased(a, zero)).context("handing the writer sound")?;
            sound.next();
        }
        if let Some(caps) = &cf.caps {
            if video.caps().as_ref() != Some(caps) {
                video.set_caps(Some(caps));
            }
        }
        video.push_buffer(rebased(&cf.frame, zero)).context("handing the writer a picture")?;
    }
    if let Some(src) = audio {
        for a in sound {
            src.push_buffer(rebased(a, zero)).context("handing the writer sound")?;
        }
        let _ = src.end_of_stream();
    }
    let _ = video.end_of_stream();
    Ok(())
}

/// Until the muxer has written its last box, or a minute more than the clip.
fn wait(pipeline: &gst::Pipeline, length: u64) -> Result<()> {
    let bus = pipeline.bus().context("a pipeline with no bus")?;
    let limit = gst::ClockTime::from_nseconds(length) + gst::ClockTime::from_seconds(60);
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_nanos(limit.nseconds()) {
        let Some(msg) = bus.timed_pop_filtered(gst::ClockTime::from_mseconds(200), &[gst::MessageType::Eos, gst::MessageType::Error]) else {
            continue;
        };
        if let gst::MessageView::Error(e) = msg.view() {
            bail!("the replay writer failed: {}", e.error());
        }
        return Ok(());
    }
    bail!("the replay writer did not finish in time; the disk may be full or very slow")
}
