//! A stream is planned only once its bit rates are measured. The report this
//! is for: a channel fed by the programme, whose first report states its
//! frame rate before a second of it has been measured, planned a Custom
//! destination's AAC 128 kbit/s against a sound rate of 0, copied the
//! programme's 160, and kept the copy.

use super::*;

/// The programme as the listener first reports it, before a whole second of
/// it is measured, and once it is.
fn programme(measured: bool) -> Live {
    let kbps = |n: u32| if measured { n } else { 0 };
    Live::from_plugin(&json!({
        "channel": "church", "app": "church", "stream": "main", "since_ms": 1, "measured": measured,
        "video": {"codec": "h264", "width": 1920, "height": 1080, "fps": 0.0, "kbps": kbps(6000), "frame_rate": 30.0},
        "audio": {"codec": "aac", "channels": 2, "sample_rate": 48000, "kbps": kbps(160)},
    }))
    .map(|mut l| {
        l.channel = "church".into();
        l
    })
    .unwrap()
}

fn custom_720p() -> Option<Value> {
    Some(json!({"video": {"codec": "h264", "width": 1280, "height": 720, "fps": {"num": 30, "den": 1}, "bitrate_kbps": 3000, "keyframe_ms": 2000},
        "audio": {"codec": "aac", "bitrate_kbps": 128}}))
}

#[test]
fn a_stream_whose_rates_are_not_measured_yet_is_not_planned() {
    let t = Transcode::with(machine(false), governor(8));
    let dests = vec![dest("custom", custom_720p())];
    replan(&t, &dests, &[programme(false)]);
    assert!(matches!(t.state.lock().outcome("church", "custom"), Some(Outcome::Waiting)), "planned on a rate of 0");
    assert_eq!(t.row("church", &dests[0], base(&dests[0])).unwrap()["rendition"], true, "and the listener waits rather than copy");
}

#[test]
fn sound_over_the_asked_rate_is_encoded_once_the_rate_is_measured() {
    let t = Transcode::with(machine(false), governor(8));
    let dests = vec![dest("custom", custom_720p())];
    replan(&t, &dests, &[programme(false)]);
    replan(&t, &dests, &[programme(true)]);
    let (plan, no) = t.view("church", "custom");
    assert!(no.is_none(), "{no:?}");
    let plan = plan.expect("planned once measured");
    let audio = plan.audio.expect("sound goes out");
    assert_eq!(audio.bitrate_kbps, 128, "the 128 asked, not the programme's 160: {plan:?}");
    assert!(plan.nodes.iter().any(|n| n.starts_with("aencode:")), "{:?}", plan.nodes);
    assert_eq!((plan.video.unwrap().width, plan.video.unwrap().height), (1280, 720));
}

#[test]
fn a_listener_that_does_not_say_is_planned_at_once() {
    let mut live = programme(true);
    live.measured = None;
    let t = Transcode::with(machine(false), governor(8));
    replan(&t, &[dest("custom", custom_720p())], &[live]);
    assert!(t.view("church", "custom").0.is_some());
}
