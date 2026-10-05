//! VP8 from a WHIP publisher, made into H.264 for the hub.
//!
//! ```text
//!   rtpvp8depay ──► vp8dec ──► videoconvert ──► x264enc or openh264enc ──► h264parse
//! ```
//!
//! H.264 stays the codec every publisher is asked for, because it is carried
//! to the hub as it came, with nothing decoded. Some Android browsers offer
//! only VP8, and turning them away is worse than the one decode and encode
//! this costs, which is paid only for a publisher that sent VP8
//! (dev/plans/browser-devices-and-guests.md, Decisions). x264 is used where the operator has installed it; openh264,
//! which is BSD licensed and ships in every build, otherwise.

use gstreamer as gst;

/// What the VP8 path needs besides an H.264 encoder. Read by `dev/gst_trim.py`,
/// so the bundled runtime keeps them.
pub const VP8_NEEDED: &[&str] = &["rtpvp8depay", "vp8dec", "videoconvert", "h264parse", "openh264enc"];

/// The encoders tried, in order.
const ENCODERS: &[&str] = &["x264enc", "openh264enc"];

/// The bit rate the encoder is given, in kbit/s: what the publisher page
/// asks a browser for, so the picture costs the hub what an H.264 one would.
const KBPS: u32 = 2500;

/// One keyframe every two seconds at 30 frames a second, so a reader that
/// joins late waits no longer than it would for a browser's own H.264.
const GOP: u32 = 60;

/// Can this machine take VP8 at all? Asked before VP8 goes in the answer, so
/// a mixer without the elements still refuses with a sentence that says so.
pub fn available() -> bool {
    let decode = ["rtpvp8depay", "vp8dec", "videoconvert", "h264parse"];
    decode.iter().all(|e| gmx_netkit::elements::exists(e)) && encoder_name().is_some()
}

/// The video the answer may take: H.264 always, VP8 too when `vp8`. H.264
/// is listed first, so a publisher offering both is answered with it and
/// its picture is never decoded.
pub fn video_caps(vp8: bool) -> String {
    let h264 = "application/x-rtp,media=video,encoding-name=H264,clock-rate=90000";
    if vp8 {
        format!("{h264};application/x-rtp,media=video,encoding-name=VP8,clock-rate=90000")
    } else {
        h264.to_string()
    }
}

fn encoder_name() -> Option<&'static str> {
    ENCODERS.iter().copied().find(|e| gmx_netkit::elements::exists(e))
}

/// The elements from the depayloader to the parser, made and configured,
/// unlinked. None when one of them cannot be made.
pub fn chain() -> Option<Vec<gst::Element>> {
    let make = |name: &str| gst::ElementFactory::make(name).build().ok();
    let depay = make("rtpvp8depay")?;
    // A packet lost asks the browser for a keyframe, as the H.264 path does.
    if depay.find_property("request-keyframe").is_some() {
        depay.set_property("request-keyframe", true);
    }
    let encoder = encoder()?;
    Some(vec![depay, make("vp8dec")?, make("videoconvert")?, encoder, make("h264parse")?])
}

/// The H.264 encoder, set for a live camera: no B frames, no look ahead.
fn encoder() -> Option<gst::Element> {
    let name = encoder_name()?;
    let e = gst::ElementFactory::make(name).build().ok()?;
    let set = |key: &str, value: &str| {
        if e.find_property(key).is_some() {
            e.set_property_from_str(key, value);
        }
    };
    if name == "x264enc" {
        set("tune", "zerolatency");
        set("speed-preset", "veryfast");
        set("bitrate", &KBPS.to_string());
        set("key-int-max", &GOP.to_string());
        set("bframes", "0");
    } else {
        set("usage-type", "camera");
        set("complexity", "low");
        set("rate-control", "bitrate");
        set("bitrate", &(KBPS * 1000).to_string());
        set("gop-size", &GOP.to_string());
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vp8_path_is_built_whole_where_the_elements_are_here() {
        gmx_netkit::init().unwrap();
        if !available() {
            eprintln!("skipped: this machine has no VP8 decoder or no H.264 encoder");
            return;
        }
        let names: Vec<String> = chain().expect("every element makes").iter().map(|e| e.factory().unwrap().name().to_string()).collect();
        assert_eq!(names[..3], ["rtpvp8depay", "vp8dec", "videoconvert"]);
        assert!(ENCODERS.contains(&names[3].as_str()), "{names:?}");
        assert_eq!(names[4], "h264parse");
    }
}
