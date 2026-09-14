//! Getting pictures and sound out of a plugin.
//!
//! The media contract is one paragraph long and it is worth repeating here,
//! because every writer in this module honours it and none of them negotiates:
//!
//! ```text
//! video   I420, BT.709, canvas width x height, canvas fps, one frame per buffer
//!         AYUV when the provide declares alpha = true
//! audio   F32LE interleaved, 48 kHz, 2 channels, 10 ms per buffer
//! time    PTS in nanoseconds on the plugin's own monotonic clock, starting near 0
//! ```
//!
//! Three transports carry it. `container` is a streamable Matroska stream on
//! stdout and works on every platform, including Windows. `unixfd` and `shm`
//! are Unix only and need the `gst` feature; they are what a plugin uses when a
//! copy per frame is too much.

pub mod matroska;

#[cfg(feature = "gst")]
pub mod gst;

use std::io::Write;

use crate::wire::{Canvas, Transport};

pub use matroska::{AudioTrack, MatroskaWriter, VideoFormat, VideoTrack};

/// What a plugin writes media through, whichever transport was negotiated.
///
/// Every method takes the PTS explicitly rather than reading a clock, so an
/// offline replay produces the same bytes every run.
pub trait MediaWriter: Send {
    /// One video frame at canvas caps. `keyframe` is true for raw video, which
    /// has no other kind of frame.
    fn write_video(&mut self, pts_ns: u64, data: &[u8], keyframe: bool) -> std::io::Result<()>;

    /// One 10 ms audio buffer, interleaved F32LE at 48 kHz.
    fn write_audio(&mut self, pts_ns: u64, data: &[u8]) -> std::io::Result<()>;

    /// Push whatever is held. The container writer holds nothing, so this is a
    /// flush of the underlying pipe.
    fn flush(&mut self) -> std::io::Result<()>;

    /// End of stream. After this the writer accepts nothing.
    fn finish(&mut self) -> std::io::Result<()>;

    /// What this writer is, for a log line.
    fn describe(&self) -> String;
}

/// What the plugin intends to send, decided before the transport is opened.
#[derive(Debug, Clone, Copy)]
pub struct Streams {
    pub video: Option<VideoFormat>,
    pub audio: bool,
}

impl Streams {
    /// Video only, which is what a clock, a scoreboard or a weather panel is.
    pub fn video_only(format: VideoFormat) -> Streams {
        Streams {
            video: Some(format),
            audio: false,
        }
    }

    /// The usual camera shape.
    pub fn video_and_audio(format: VideoFormat) -> Streams {
        Streams {
            video: Some(format),
            audio: true,
        }
    }

    pub fn audio_only() -> Streams {
        Streams {
            video: None,
            audio: true,
        }
    }
}

/// The container transport: streamable Matroska on a pipe.
///
/// This is the writer with no native dependency. It costs one copy of each
/// frame into the pipe and nothing else; at 1080p30 the pipe carries about
/// 93 MB/s, which is the number 03 section 5 quotes.
pub struct ContainerWriter<W: Write + Send> {
    inner: MatroskaWriter<W>,
    done: bool,
}

impl<W: Write + Send> ContainerWriter<W> {
    pub fn new(out: W, canvas: Canvas, streams: Streams) -> std::io::Result<Self> {
        let video = streams.video.map(|format| VideoTrack {
            width: canvas.width,
            height: canvas.height,
            fps: canvas.fps,
            format,
        });
        let audio = streams.audio.then(AudioTrack::default);
        let mut inner = MatroskaWriter::new(out, video, audio);
        // The header goes out before the first frame is drawn, so a slow first
        // draw does not look like a plugin that never started.
        inner.write_header()?;
        Ok(ContainerWriter { inner, done: false })
    }
}

impl<W: Write + Send> MediaWriter for ContainerWriter<W> {
    fn write_video(&mut self, pts_ns: u64, data: &[u8], keyframe: bool) -> std::io::Result<()> {
        if self.done {
            return Err(closed());
        }
        self.inner.write_video(pts_ns, data, keyframe)
    }

    fn write_audio(&mut self, pts_ns: u64, data: &[u8]) -> std::io::Result<()> {
        if self.done {
            return Err(closed());
        }
        self.inner.write_audio(pts_ns, data)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }

    fn finish(&mut self) -> std::io::Result<()> {
        self.done = true;
        self.inner.flush()
    }

    fn describe(&self) -> String {
        "container (streamable Matroska on stdout, written by the SDK)".into()
    }
}

fn closed() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "the media writer was finished. Call start again to open a new one.",
    )
}

/// Open the container transport on this process's stdout.
///
/// In container mode stdout is media and nothing else. A plugin that prints to
/// stdout corrupts its own stream, which is why the SDK takes the handle.
pub fn open_stdout(
    canvas: Canvas,
    streams: Streams,
) -> std::io::Result<ContainerWriter<std::io::BufWriter<std::io::Stdout>>> {
    // A buffer the size of one 1080p frame, so a frame is one write.
    let out = std::io::BufWriter::with_capacity(4 << 20, std::io::stdout());
    ContainerWriter::new(out, canvas, streams)
}

/// Open whichever transport the core chose in the handshake.
///
/// Without the `gst` feature only `container` is available, and the error for
/// the other two names the feature. With it, on Windows, `unixfd` and `shm`
/// still name the platform, because neither element exists there.
pub fn open(
    transport: Transport,
    address: &str,
    canvas: Canvas,
    streams: Streams,
) -> std::io::Result<Box<dyn MediaWriter>> {
    match transport {
        Transport::Container => Ok(Box::new(open_stdout(canvas, streams)?)),
        Transport::Unixfd | Transport::Shm => {
            open_fd_transport(transport, address, canvas, streams)
        }
    }
}

#[cfg(all(unix, feature = "gst"))]
fn open_fd_transport(
    transport: Transport,
    address: &str,
    canvas: Canvas,
    streams: Streams,
) -> std::io::Result<Box<dyn MediaWriter>> {
    let writer = gst::FdTransportWriter::new(transport, address, canvas, streams)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(Box::new(writer))
}

#[cfg(not(all(unix, feature = "gst")))]
fn open_fd_transport(
    transport: Transport,
    _address: &str,
    _canvas: Canvas,
    _streams: Streams,
) -> std::io::Result<Box<dyn MediaWriter>> {
    let why = if cfg!(unix) {
        "this build of the SDK has no GStreamer: build it with --features gst"
    } else {
        "unixfdsink and shmsink do not exist on this platform"
    };
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!(
            "the core chose the '{}' transport and {why}. \
             Declare transports = [\"container\"] in gmx-plugin.toml; a container on a pipe \
             works everywhere and costs one copy per frame.",
            transport.as_str()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_container_writer_emits_a_header_before_any_frame() {
        let canvas = Canvas::new(32, 32, 30);
        let w = ContainerWriter::new(Vec::new(), canvas, Streams::video_only(VideoFormat::I420))
            .unwrap();
        assert!(w.describe().contains("container"));
    }

    #[test]
    fn a_finished_writer_refuses_more_frames() {
        let canvas = Canvas::new(32, 32, 30);
        let mut w =
            ContainerWriter::new(Vec::new(), canvas, Streams::video_only(VideoFormat::I420))
                .unwrap();
        let frame = vec![0u8; canvas.i420_frame_bytes()];
        w.write_video(0, &frame, true).unwrap();
        w.finish().unwrap();
        let err = w.write_video(33_333_333, &frame, true).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe);
    }

    #[cfg(not(all(unix, feature = "gst")))]
    #[test]
    fn the_fd_transports_name_the_way_out() {
        let err = match open(
            Transport::Unixfd,
            "/run/gmx/cam1.sock",
            Canvas::new(32, 32, 30),
            Streams::video_only(VideoFormat::I420),
        ) {
            Ok(_) => panic!("unixfd must not open without GStreamer"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
        assert!(err.to_string().contains("container"), "{err}");
    }

    #[test]
    fn streams_shapes_say_what_they_carry() {
        assert!(Streams::video_only(VideoFormat::I420).video.is_some());
        assert!(!Streams::video_only(VideoFormat::I420).audio);
        assert!(Streams::audio_only().video.is_none());
        assert!(Streams::video_and_audio(VideoFormat::Ayuv).audio);
    }
}
