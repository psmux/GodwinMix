//! Ending a capture chain at the transport the core negotiated.
//!
//! A capture plugin is one GStreamer pipeline. The front of it differs per
//! plugin and per platform; the back of it is the same every time, and getting
//! it wrong is silent: the core waits on a socket nobody binds, or reads a
//! container it cannot demux, and the operator sees a black tile.
//!
//! Two shapes:
//!
//! ```text
//! container   <video> ! queue ! mux.   <audio> ! queue ! mux.
//!             matroskamux name=mux streamable=true ! fdsink fd=1
//!
//! unixfd/shm  <video> ! queue ! unixfdsink socket-path=<base>.video
//!             <audio> ! queue ! unixfdsink socket-path=<base>.audio
//! ```
//!
//! The `.video` and `.audio` suffixes are not a choice. `MediaDir` in
//! `crates/godwinmix-core/src/plugin/host/transport.rs` builds exactly those
//! two names from the base address it sends in the handshake, and
//! `docs/reference/plugin-lifecycle.md` says so too.
//!
//! Socket paths are set on the elements after the description is parsed, not
//! written into it. A runtime directory with a space in it would otherwise
//! produce a parse error that names neither the space nor the path.

use gstreamer as gst;
use gstreamer::prelude::*;

use godwinmix_sdk::wire::Transport;

/// The name the video sink carries in every pipeline this module builds.
pub const VIDEO_SINK: &str = "gmx-video-sink";
/// The name the audio sink carries in every pipeline this module builds.
pub const AUDIO_SINK: &str = "gmx-audio-sink";

/// The socket the core reads this plugin's video from.
pub fn video_socket(base: &str) -> String {
    format!("{base}.video")
}

/// The socket the core reads this plugin's audio from.
pub fn audio_socket(base: &str) -> String {
    format!("{base}.audio")
}

/// The chains a plugin wants carried, without their sinks.
///
/// Each string is a `gst-launch` fragment ending at the element whose output
/// is already at the canvas contract: I420 at canvas size and rate for video,
/// F32LE 48 kHz stereo for audio.
#[derive(Debug, Default, Clone)]
pub struct Wiring {
    pub video: Option<String>,
    pub audio: Option<String>,
}

impl Wiring {
    pub fn video_only(chain: impl Into<String>) -> Wiring {
        Wiring {
            video: Some(chain.into()),
            audio: None,
        }
    }

    pub fn audio_only(chain: impl Into<String>) -> Wiring {
        Wiring {
            video: None,
            audio: Some(chain.into()),
        }
    }

    /// The whole pipeline description, sinks included.
    ///
    /// Socket paths are left unset; [`bind`] fills them in once the pipeline
    /// exists.
    pub fn description(&self, transport: Transport) -> Result<String, String> {
        if self.video.is_none() && self.audio.is_none() {
            return Err(
                "a capture pipeline with neither video nor audio carries nothing. \
                        Declare one of them in the manifest's media table."
                    .into(),
            );
        }
        Ok(match transport {
            Transport::Container => self.container(),
            Transport::Unixfd => self.sockets("unixfdsink"),
            Transport::Shm => self.sockets("shmsink"),
        })
    }

    /// Streamable Matroska on stdout, which is what `container` means.
    fn container(&self) -> String {
        let mut parts = vec![
            "matroskamux name=gmx-mux streamable=true ! fdsink name=gmx-fd fd=1 sync=false \
             async=false"
                .to_string(),
        ];
        if let Some(video) = &self.video {
            // Leaky, because a core that stops reading must slow the pipe and
            // never the camera. A dropped raw frame costs one picture; a
            // blocked capture thread costs the source.
            parts.push(format!(
                "{video} ! queue name=gmx-video-queue max-size-buffers=4 max-size-bytes=0 \
                 max-size-time=0 leaky=downstream ! gmx-mux."
            ));
        }
        if let Some(audio) = &self.audio {
            parts.push(format!(
                "{audio} ! queue name=gmx-audio-queue max-size-buffers=64 max-size-bytes=0 \
                 max-size-time=0 ! gmx-mux."
            ));
        }
        parts.join(" ")
    }

    /// One socket per stream, which is what `unixfd` and `shm` mean.
    fn sockets(&self, sink: &str) -> String {
        let mut parts = Vec::new();
        if let Some(video) = &self.video {
            parts.push(format!(
                "{video} ! queue name=gmx-video-queue max-size-buffers=4 max-size-bytes=0 \
                 max-size-time=0 leaky=downstream ! {sink} name={VIDEO_SINK} sync=false"
            ));
        }
        if let Some(audio) = &self.audio {
            parts.push(format!(
                "{audio} ! queue name=gmx-audio-queue max-size-buffers=64 max-size-bytes=0 \
                 max-size-time=0 ! {sink} name={AUDIO_SINK} sync=false"
            ));
        }
        parts.join(" ")
    }
}

/// Put the negotiated addresses on the sinks the description named.
///
/// A no-op for the container transport, where stdout is the address. For the
/// socket transports an empty base is refused here rather than at the sink,
/// which reports it as a null property three state changes later.
pub fn bind(pipeline: &gst::Pipeline, transport: Transport, base: &str) -> Result<(), String> {
    if transport == Transport::Container {
        return Ok(());
    }
    if base.is_empty() {
        return Err(format!(
            "the core chose the '{}' transport and sent no address. This is a core bug; \
             declare transports = [\"container\"] in gmx-plugin.toml to work around it.",
            transport.as_str()
        ));
    }
    for (name, path) in [
        (VIDEO_SINK, video_socket(base)),
        (AUDIO_SINK, audio_socket(base)),
    ] {
        if let Some(sink) = pipeline.by_name(name) {
            sink.set_property("socket-path", &path);
            // shmsink alone needs a size; unixfdsink passes file descriptors
            // and has no such property, so this is set only where it exists.
            crate::elements::set_number(&sink, "shm-size", 64 * 1024 * 1024);
            crate::elements::set_flag(&sink, "wait-for-connection", false);
        }
    }
    Ok(())
}

/// The caps one video frame must carry, as a `gst-launch` fragment.
///
/// `pixel-aspect-ratio` is pinned because a camera that reports 10/11 makes
/// `videoscale` produce a picture the compositor then letterboxes.
pub fn canvas_video_caps(width: u32, height: u32, fps: u32) -> String {
    format!(
        "video/x-raw,format=I420,width={width},height={height},framerate={fps}/1,\
         pixel-aspect-ratio=1/1,colorimetry=bt709"
    )
}

/// The caps one audio buffer must carry.
pub fn canvas_audio_caps() -> &'static str {
    "audio/x-raw,format=F32LE,rate=48000,channels=2,layout=interleaved"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_names_are_the_ones_the_core_reads() {
        assert_eq!(
            video_socket("/run/gmx/cam1/media"),
            "/run/gmx/cam1/media.video"
        );
        assert_eq!(
            audio_socket("/run/gmx/cam1/media"),
            "/run/gmx/cam1/media.audio"
        );
    }

    #[test]
    fn a_container_description_muxes_both_streams_into_one_pipe() {
        let w = Wiring {
            video: Some("videotestsrc ! videoconvert".into()),
            audio: Some("audiotestsrc ! audioconvert".into()),
        };
        let d = w.description(Transport::Container).unwrap();
        assert!(
            d.contains("matroskamux name=gmx-mux streamable=true"),
            "{d}"
        );
        assert!(d.contains("fdsink name=gmx-fd fd=1"), "{d}");
        assert_eq!(d.matches("gmx-mux.").count(), 2, "{d}");
        assert!(!d.contains("unixfdsink"), "{d}");
    }

    #[test]
    fn a_socket_description_gives_each_stream_its_own_sink() {
        let w = Wiring {
            video: Some("videotestsrc".into()),
            audio: Some("audiotestsrc".into()),
        };
        let d = w.description(Transport::Unixfd).unwrap();
        assert!(d.contains(&format!("unixfdsink name={VIDEO_SINK}")), "{d}");
        assert!(d.contains(&format!("unixfdsink name={AUDIO_SINK}")), "{d}");
        assert!(!d.contains("matroskamux"), "{d}");
    }

    #[test]
    fn an_audio_only_source_builds_no_video_branch() {
        let d = Wiring::audio_only("audiotestsrc")
            .description(Transport::Container)
            .unwrap();
        assert!(!d.contains("gmx-video-queue"), "{d}");
        assert!(d.contains("gmx-audio-queue"), "{d}");
    }

    #[test]
    fn a_wiring_that_carries_nothing_says_so() {
        let err = Wiring::default()
            .description(Transport::Container)
            .unwrap_err();
        assert!(err.contains("media table"), "{err}");
    }

    #[test]
    fn binding_a_socket_transport_with_no_address_names_the_container() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let err = bind(&pipeline, Transport::Unixfd, "").unwrap_err();
        assert!(err.contains("container"), "{err}");
        bind(&pipeline, Transport::Container, "").expect("stdout needs no address");
    }

    #[test]
    fn binding_puts_the_address_on_the_sink_the_description_named() {
        gst::init().unwrap();
        let description = Wiring::audio_only("audiotestsrc")
            .description(Transport::Unixfd)
            .expect("a description");
        let pipeline = crate::capture::build(&description).expect("it parses");
        bind(&pipeline, Transport::Unixfd, "/tmp/gmx-test/media").expect("it binds");
        let sink = pipeline
            .by_name(AUDIO_SINK)
            .expect("the audio sink is named");
        assert_eq!(
            sink.property::<Option<String>>("socket-path").as_deref(),
            Some("/tmp/gmx-test/media.audio")
        );
    }

    #[test]
    fn the_canvas_caps_are_the_media_contract() {
        let caps = canvas_video_caps(1280, 720, 30);
        assert!(caps.contains("format=I420"));
        assert!(caps.contains("width=1280"));
        assert!(caps.contains("framerate=30/1"));
        assert!(canvas_audio_caps().contains("rate=48000"));
    }
}
