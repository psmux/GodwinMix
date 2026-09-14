//! The GStreamer backed writers, behind the `gst` feature.
//!
//! Two reasons to turn this on. One: a plugin that wants the `unixfd` or `shm`
//! transport needs `unixfdsink` or `shmsink`, and there is no way to write
//! either from pure Rust. Two: a plugin already linking GStreamer for its own
//! reasons may as well mux with `matroskamux` rather than carry the SDK's
//! writer as well.
//!
//! Everything here is optional. Without the feature the crate has no native
//! dependency and the container transport still works.

// Only the fd transports need these, and they are Unix only.
#[cfg(unix)]
use std::sync::Mutex;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;

use crate::media::{MediaWriter, Streams, VideoFormat};
use crate::wire::Canvas;
#[cfg(unix)]
use crate::wire::Transport;

/// What can go wrong building or feeding a GStreamer pipeline.
#[derive(Debug)]
pub enum GstError {
    Init(String),
    Build(String),
    Push(String),
}

impl std::fmt::Display for GstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GstError::Init(s) => write!(f, "GStreamer would not start: {s}"),
            GstError::Build(s) => write!(f, "could not build the media pipeline: {s}"),
            GstError::Push(s) => write!(f, "could not push a buffer: {s}"),
        }
    }
}

impl std::error::Error for GstError {}

fn init() -> Result<(), GstError> {
    gst::init().map_err(|e| GstError::Init(e.to_string()))
}

/// The caps of one raw video frame at canvas caps.
pub fn video_caps(canvas: Canvas, format: VideoFormat) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", format.as_str())
        .field("width", canvas.width as i32)
        .field("height", canvas.height as i32)
        .field("framerate", gst::Fraction::new(canvas.fps as i32, 1))
        .field("colorimetry", "bt709")
        .build()
}

/// The caps of one 10 ms audio buffer.
pub fn audio_caps() -> gst::Caps {
    gst::Caps::builder("audio/x-raw")
        .field("format", "F32LE")
        .field("rate", 48_000i32)
        .field("channels", 2i32)
        .field("layout", "interleaved")
        .build()
}

fn appsrc(pipeline: &gst::Pipeline, name: &str) -> Result<AppSrc, GstError> {
    pipeline
        .by_name(name)
        .ok_or_else(|| GstError::Build(format!("the pipeline has no element named '{name}'")))?
        .downcast::<AppSrc>()
        .map_err(|_| GstError::Build(format!("'{name}' is not an appsrc")))
}

fn push(src: &AppSrc, pts_ns: u64, data: &[u8], keyframe: bool) -> std::io::Result<()> {
    let mut buffer = gst::Buffer::from_mut_slice(data.to_vec());
    {
        let b = buffer.get_mut().expect("the buffer was just made");
        b.set_pts(gst::ClockTime::from_nseconds(pts_ns));
        b.set_dts(gst::ClockTime::from_nseconds(pts_ns));
        if !keyframe {
            b.set_flags(gst::BufferFlags::DELTA_UNIT);
        }
    }
    src.push_buffer(buffer)
        .map(|_| ())
        .map_err(|e| std::io::Error::other(GstError::Push(e.to_string()).to_string()))
}

/// Mux with `matroskamux` and write the result to a file descriptor.
///
/// The same bytes as the SDK's own writer produces, made by GStreamer instead.
/// Useful when a plugin is already linking GStreamer, and as the thing the
/// pure Rust writer is tested against.
pub struct GstContainerWriter {
    pipeline: gst::Pipeline,
    video: Option<AppSrc>,
    audio: Option<AppSrc>,
    done: bool,
}

impl GstContainerWriter {
    /// Write to this file descriptor. 1 is stdout, which is what the container
    /// transport means.
    pub fn new(fd: i32, canvas: Canvas, streams: Streams) -> Result<Self, GstError> {
        init()?;
        let mut description = format!(
            "matroskamux name=mux streamable=true ! fdsink fd={fd} sync=false async=false"
        );
        if streams.video.is_some() {
            description.push_str(" appsrc name=v is-live=true format=time do-timestamp=false ! queue max-size-buffers=3 ! mux.");
        }
        if streams.audio {
            description.push_str(" appsrc name=a is-live=true format=time do-timestamp=false ! queue max-size-buffers=8 ! mux.");
        }
        let pipeline = gst::parse::launch(&description)
            .map_err(|e| GstError::Build(e.to_string()))?
            .downcast::<gst::Pipeline>()
            .map_err(|_| GstError::Build("parse did not produce a pipeline".into()))?;

        let video = match streams.video {
            Some(format) => {
                let src = appsrc(&pipeline, "v")?;
                src.set_caps(Some(&video_caps(canvas, format)));
                Some(src)
            }
            None => None,
        };
        let audio = if streams.audio {
            let src = appsrc(&pipeline, "a")?;
            src.set_caps(Some(&audio_caps()));
            Some(src)
        } else {
            None
        };
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| GstError::Build(e.to_string()))?;
        Ok(GstContainerWriter {
            pipeline,
            video,
            audio,
            done: false,
        })
    }
}

impl MediaWriter for GstContainerWriter {
    fn write_video(&mut self, pts_ns: u64, data: &[u8], keyframe: bool) -> std::io::Result<()> {
        match (&self.video, self.done) {
            (Some(src), false) => push(src, pts_ns, data, keyframe),
            _ => Err(no_such_stream("video")),
        }
    }

    fn write_audio(&mut self, pts_ns: u64, data: &[u8]) -> std::io::Result<()> {
        match (&self.audio, self.done) {
            (Some(src), false) => push(src, pts_ns, data, true),
            _ => Err(no_such_stream("audio")),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn finish(&mut self) -> std::io::Result<()> {
        if self.done {
            return Ok(());
        }
        self.done = true;
        for src in [self.video.as_ref(), self.audio.as_ref()].into_iter().flatten() {
            let _ = src.end_of_stream();
        }
        // Wait briefly for the mux to drain, then stop. A plugin shutting down
        // has eight seconds; this uses a fraction of one.
        let bus = self.pipeline.bus();
        if let Some(bus) = bus {
            let _ = bus.timed_pop_filtered(
                gst::ClockTime::from_mseconds(500),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
        }
        let _ = self.pipeline.set_state(gst::State::Null);
        Ok(())
    }

    fn describe(&self) -> String {
        "container (streamable Matroska written by GStreamer matroskamux)".into()
    }
}

impl Drop for GstContainerWriter {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

fn no_such_stream(which: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        format!(
            "this writer carries no {which}, or it was finished. Declare it in the manifest's \
             media table and open the writer again."
        ),
    )
}

/// Raw frames over `unixfdsink` or `shmsink`.
///
/// Unix only, because neither element exists anywhere else. Video and audio get
/// a socket each: the address the core sends is the video socket, and the audio
/// socket is that path with `.audio` on the end.
#[cfg(unix)]
pub struct FdTransportWriter {
    video: Option<(gst::Pipeline, AppSrc)>,
    audio: Option<(gst::Pipeline, AppSrc)>,
    transport: Transport,
    done: Mutex<bool>,
}

#[cfg(unix)]
impl FdTransportWriter {
    pub fn new(
        transport: Transport,
        address: &str,
        canvas: Canvas,
        streams: Streams,
    ) -> Result<Self, GstError> {
        init()?;
        if address.is_empty() {
            return Err(GstError::Build(format!(
                "the '{}' transport needs a socket path and the core sent none. \
                 Declare transports = [\"container\"] to work without one.",
                transport.as_str()
            )));
        }
        let video = match streams.video {
            Some(format) => {
                let (pipeline, src) = Self::branch(transport, address, "v")?;
                src.set_caps(Some(&video_caps(canvas, format)));
                pipeline
                    .set_state(gst::State::Playing)
                    .map_err(|e| GstError::Build(e.to_string()))?;
                Some((pipeline, src))
            }
            None => None,
        };
        let audio = if streams.audio {
            let (pipeline, src) = Self::branch(transport, &format!("{address}.audio"), "a")?;
            src.set_caps(Some(&audio_caps()));
            pipeline
                .set_state(gst::State::Playing)
                .map_err(|e| GstError::Build(e.to_string()))?;
            Some((pipeline, src))
        } else {
            None
        };
        Ok(FdTransportWriter {
            video,
            audio,
            transport,
            done: Mutex::new(false),
        })
    }

    fn branch(
        transport: Transport,
        socket: &str,
        name: &str,
    ) -> Result<(gst::Pipeline, AppSrc), GstError> {
        let sink = match transport {
            Transport::Unixfd => format!("unixfdsink socket-path={socket}"),
            Transport::Shm => format!(
                "shmsink socket-path={socket} wait-for-connection=false shm-size=67108864 sync=false"
            ),
            Transport::Container => {
                return Err(GstError::Build(
                    "the container transport does not use a socket".into(),
                ))
            }
        };
        let description =
            format!("appsrc name={name} is-live=true format=time do-timestamp=false ! {sink}");
        let pipeline = gst::parse::launch(&description)
            .map_err(|e| GstError::Build(format!("{e}. Is gst-plugins-bad installed?")))?
            .downcast::<gst::Pipeline>()
            .map_err(|_| GstError::Build("parse did not produce a pipeline".into()))?;
        let src = appsrc(&pipeline, name)?;
        Ok((pipeline, src))
    }
}

#[cfg(unix)]
impl MediaWriter for FdTransportWriter {
    fn write_video(&mut self, pts_ns: u64, data: &[u8], keyframe: bool) -> std::io::Result<()> {
        match &self.video {
            Some((_, src)) => push(src, pts_ns, data, keyframe),
            None => Err(no_such_stream("video")),
        }
    }

    fn write_audio(&mut self, pts_ns: u64, data: &[u8]) -> std::io::Result<()> {
        match &self.audio {
            Some((_, src)) => push(src, pts_ns, data, true),
            None => Err(no_such_stream("audio")),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn finish(&mut self) -> std::io::Result<()> {
        let mut done = self.done.lock().unwrap_or_else(|e| e.into_inner());
        if *done {
            return Ok(());
        }
        *done = true;
        for (pipeline, src) in [self.video.as_ref(), self.audio.as_ref()].into_iter().flatten() {
            let _ = src.end_of_stream();
            let _ = pipeline.set_state(gst::State::Null);
        }
        Ok(())
    }

    fn describe(&self) -> String {
        format!("{} (raw frames, GStreamer)", self.transport.as_str())
    }
}

#[cfg(unix)]
impl Drop for FdTransportWriter {
    fn drop(&mut self) {
        for (pipeline, _) in [self.video.as_ref(), self.audio.as_ref()].into_iter().flatten() {
            let _ = pipeline.set_state(gst::State::Null);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_caps_state_the_contract() {
        init().unwrap();
        let caps = video_caps(Canvas::new(1280, 720, 30), VideoFormat::I420);
        let s = caps.structure(0).unwrap();
        assert_eq!(s.name(), "video/x-raw");
        assert_eq!(s.get::<String>("format").unwrap(), "I420");
        assert_eq!(s.get::<i32>("width").unwrap(), 1280);
        assert_eq!(
            s.get::<gst::Fraction>("framerate").unwrap(),
            gst::Fraction::new(30, 1)
        );
    }

    #[test]
    fn audio_caps_state_the_contract() {
        init().unwrap();
        let caps = audio_caps();
        let s = caps.structure(0).unwrap();
        assert_eq!(s.get::<String>("format").unwrap(), "F32LE");
        assert_eq!(s.get::<i32>("rate").unwrap(), 48_000);
        assert_eq!(s.get::<i32>("channels").unwrap(), 2);
    }

    #[test]
    fn an_alpha_source_asks_for_ayuv() {
        init().unwrap();
        let caps = video_caps(Canvas::new(64, 64, 25), VideoFormat::Ayuv);
        assert_eq!(
            caps.structure(0).unwrap().get::<String>("format").unwrap(),
            "AYUV"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_fd_transport_with_no_address_says_what_to_do_instead() {
        let err = match FdTransportWriter::new(
            Transport::Shm,
            "",
            Canvas::new(64, 64, 30),
            Streams::video_only(VideoFormat::I420),
        ) {
            Ok(_) => panic!("an empty socket path must not open a transport"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("container"), "{err}");
    }
}
