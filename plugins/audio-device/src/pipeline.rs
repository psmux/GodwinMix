//! Building the input's pipeline.
//!
//! ```text
//!  <input> ! audioconvert ! audioresample !
//!            audio/x-raw,format=F32LE,rate=48000,channels=2,layout=interleaved !
//!            audiorate ! volume ! audiobuffersplit ! queue ! <transport>
//! ```
//!
//! `audiorate` is there because a sound card's clock is not the canvas's and a
//! gap in the samples must be filled with silence rather than with a jump.
//! `audiobuffersplit` is there because the media contract asks for ten
//! milliseconds a buffer and a driver that hands over forty would otherwise
//! decide the mixer's audio latency for it.

use gstreamer as gst;
use gstreamer::prelude::*;

use godwinmix_capture_common::{capture, devices, elements, wiring};
use godwinmix_sdk::wire::Transport;

use crate::settings::Settings;

/// The element the input is attached to.
const HEAD: &str = "gmx-head";
/// The capture element.
pub const SOURCE: &str = "gmx-src";
/// The gain and mute element, changed while the input runs.
pub const VOLUME: &str = "gmx-volume";

/// The capture elements to try, in order, on this platform.
///
/// Linux leads with PipeWire because that is what a current desktop runs and
/// because it is the only one that can take an input another program already
/// has. PulseAudio is the fallback on an older machine and ALSA the one under
/// both. macOS has Core Audio and nothing else. Windows leads with WASAPI 2;
/// `wasapi2src` has open stutter bugs (gstreamer#2870 and #3339) and
/// `directsoundsrc` is the older path to fall back to when they bite.
pub const CANDIDATES: &[&str] = if cfg!(target_os = "linux") {
    &["pipewiresrc", "pulsesrc", "alsasrc"]
} else if cfg!(target_os = "macos") {
    &["osxaudiosrc"]
} else if cfg!(target_os = "windows") {
    &["wasapi2src", "directsoundsrc"]
} else {
    &["audiotestsrc"]
};

/// The chain from the input to the canvas audio contract, without its sink.
pub fn chain() -> String {
    let mut chain = format!(
        "audioconvert name={HEAD} ! audioresample ! {} ! audiorate ! volume name={VOLUME}",
        wiring::canvas_audio_caps()
    );
    // gst-plugins-bad. Where it is missing the buffer size is whatever the
    // driver's `latency-time` produced, which is close enough to carry sound
    // and is why this is not required.
    if elements::exists("audiobuffersplit") {
        chain.push_str(" ! audiobuffersplit output-buffer-duration=1/100");
    }
    chain
}

/// The whole pipeline, with the input attached and the addresses bound.
pub fn build(
    settings: &Settings,
    transport: Transport,
    address: &str,
) -> Result<gst::Pipeline, String> {
    let description = wiring::Wiring::audio_only(chain()).description(transport)?;
    let pipeline = capture::build(&description)?;
    wiring::bind(&pipeline, transport, address)?;
    let source = open(settings)?;
    attach(&pipeline, &source)?;
    apply_gain(&pipeline, settings);
    Ok(pipeline)
}

/// Put the gain and the mute on a pipeline that is already built, or running.
pub fn apply_gain(pipeline: &gst::Pipeline, settings: &Settings) {
    if let Some(volume) = pipeline.by_name(VOLUME) {
        volume.set_property("volume", settings.linear_gain());
        // `mute` as well as a zero gain: one silences the samples, the other
        // tells anything downstream reading the property what the operator
        // asked for.
        elements::set_flag(&volume, "mute", settings.muted);
    }
}

/// Open the input the settings name.
pub fn open(settings: &Settings) -> Result<gst::Element, String> {
    if !settings.element.is_empty() {
        return by_factory(&settings.element, &settings.device);
    }
    match devices::find(devices::MICROPHONE, &settings.device) {
        Ok(found) => {
            let element = found.element(SOURCE)?;
            tune(&element);
            Ok(element)
        }
        Err(from_monitor) => {
            let factory = elements::require(
                "capturing sound",
                CANDIDATES,
                "Install the GStreamer plugin that carries it (gstreamer1.0-pipewire or \
                 gstreamer1.0-pulseaudio on Debian, gst-plugins-good in Homebrew).",
            )
            .map_err(|missing| format!("{from_monitor} {missing}"))?;
            by_factory(factory, &settings.device)
        }
    }
}

fn by_factory(factory: &str, device: &str) -> Result<gst::Element, String> {
    let element = gst::ElementFactory::make(factory)
        .name(SOURCE)
        .build()
        .map_err(|e| format!("this machine has no '{factory}' element: {e}"))?;
    if !device.is_empty() && elements::point_at(&element, device).is_none() {
        return Err(format!(
            "'{factory}' has no property that takes a device id, so '{device}' cannot be \
             selected with it. Clear the `element` setting and let the plugin choose."
        ));
    }
    tune(&element);
    Ok(element)
}

/// What every capture element here has in common.
///
/// Note what is *not* done: the driver's own buffer size is left alone.
/// Asking Core Audio for a ten millisecond device buffer (`latency-time`)
/// makes `osxaudiosrc` deliver about five buffers and then stop, measured on a
/// MacBook Pro's built in microphone, and the same request is ignored or
/// rounded by most ALSA and WASAPI drivers anyway. The media contract's ten
/// millisecond buffers are made true downstream by `audiobuffersplit`, which
/// costs one split and cannot stall a driver.
///
/// `do-timestamp` is left alone for the same kind of reason: an audio source
/// stamps its buffers from the samples it has actually read, which is exact,
/// and replacing that with a clock reading taken when the buffer was pushed is
/// a jitter `audiorate` downstream believes and answers with dropped buffers.
fn tune(_element: &gst::Element) {}

fn attach(pipeline: &gst::Pipeline, source: &gst::Element) -> Result<(), String> {
    let head = pipeline
        .by_name(HEAD)
        .ok_or_else(|| format!("the pipeline has no '{HEAD}' to attach the input to"))?;
    pipeline
        .add(source)
        .map_err(|e| format!("could not put the input in the pipeline: {e}"))?;
    source
        .link(&head)
        .map_err(|e| format!("the input and the pipeline would not agree on a format: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_chain_ends_at_the_canvas_audio_contract() {
        godwinmix_capture_common::init().unwrap();
        let chain = chain();
        assert!(chain.contains("format=F32LE"), "{chain}");
        assert!(chain.contains("rate=48000"), "{chain}");
        assert!(chain.contains("channels=2"), "{chain}");
        assert!(chain.contains(&format!("volume name={VOLUME}")), "{chain}");
    }

    #[test]
    fn ten_millisecond_buffers_are_asked_for_where_the_element_exists() {
        godwinmix_capture_common::init().unwrap();
        if elements::exists("audiobuffersplit") {
            assert!(
                chain().contains("output-buffer-duration=1/100"),
                "{}",
                chain()
            );
        }
    }

    #[test]
    fn this_platform_has_a_capture_element_named_for_it() {
        if cfg!(target_os = "linux") {
            assert_eq!(CANDIDATES, &["pipewiresrc", "pulsesrc", "alsasrc"]);
        }
        if cfg!(target_os = "macos") {
            assert_eq!(CANDIDATES, &["osxaudiosrc"]);
        }
        if cfg!(target_os = "windows") {
            assert_eq!(
                CANDIDATES,
                &["wasapi2src", "directsoundsrc"],
                "the fallback must stay"
            );
        }
    }

    #[test]
    fn a_test_tone_builds_a_whole_pipeline_on_any_machine() {
        godwinmix_capture_common::init().unwrap();
        let settings = Settings::from(&json!({"element": "audiotestsrc"}));
        let pipeline =
            build(&settings, Transport::Container, "").expect("a test tone builds everywhere");
        assert!(pipeline.by_name(SOURCE).is_some());
        assert!(pipeline.by_name(VOLUME).is_some());
        assert!(
            pipeline.by_name("gmx-video-queue").is_none(),
            "there is no picture here"
        );
    }

    /// The regression that cost an afternoon: forcing the driver's buffer
    /// size made this five buffers in three seconds instead of three hundred.
    #[test]
    #[ignore = "requires a working audio input; run explicitly on a capture machine"]
    fn the_samples_actually_flow_from_whatever_this_machine_has() {
        assert_samples_flow(&Settings::default());
    }

    #[test]
    fn test_tone_samples_flow_without_an_audio_device() {
        assert_samples_flow(&Settings::from(&json!({"element": "audiotestsrc"})));
    }

    fn assert_samples_flow(settings: &Settings) {
        use godwinmix_capture_common::{capture, Capture};
        godwinmix_capture_common::init().unwrap();
        let description = format!(
            "{} ! queue name=gmx-audio-queue ! fakesink sync=false",
            chain()
        );
        let pipeline = capture::build(&description).expect("it parses");
        let source = open(settings).expect("the configured audio source exists");
        attach(&pipeline, &source).expect("it links");
        let capture = Capture::start(pipeline, Some("gmx-audio-queue"), None).expect("it plays");
        std::thread::sleep(std::time::Duration::from_millis(2_000));
        let seen = capture.buffers();
        assert!(
            seen > 100,
            "only {seen} buffers in two seconds, and ten milliseconds each means about two \
             hundred. Something is throttling the driver.",
        );
    }

    #[test]
    fn the_gain_lands_on_the_volume_element() {
        godwinmix_capture_common::init().unwrap();
        let settings = Settings::from(&json!({"element": "audiotestsrc", "gain_db": -6.0}));
        let pipeline = build(&settings, Transport::Container, "").expect("it builds");
        let volume = pipeline.by_name(VOLUME).expect("there is a volume");
        assert!((volume.property::<f64>("volume") - 0.5012).abs() < 0.001);
        assert!(!volume.property::<bool>("mute"));

        apply_gain(
            &pipeline,
            &Settings::from(&json!({"element": "audiotestsrc", "muted": true})),
        );
        assert_eq!(volume.property::<f64>("volume"), 0.0);
        assert!(volume.property::<bool>("mute"));
    }
}
