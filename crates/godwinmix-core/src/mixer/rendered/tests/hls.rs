//! An `hls/output` with a ladder takes its rungs from the planner: no
//! encoder of its own, a rung shared with another output that wants the
//! same thing, and every rung's segments on the same numbers.

use super::{config, encoders, mixer, output, want};
use crate::config::OutputConfig;
use crate::hls::{stream, TrackKind};
use std::time::{Duration, Instant};

fn hls_output(id: &str) -> OutputConfig {
    let text = format!(
        r#"
        id = "{id}"
        type = "hls/output"
        params = {{ segment_ms = 1000, window = 6 }}
        [[rendition.ladder]]
        id = "180p"
        video = {{ height = 180, bitrate_kbps = 600 }}
        audio = {{ bitrate_kbps = 96 }}
        [[rendition.ladder]]
        id = "144p"
        video = {{ codec = "h264", height = 144, fps = {{ num = 30, den = 1 }}, bitrate_kbps = 400, keyframe_ms = 1000 }}
        audio = {{ bitrate_kbps = 96 }}
        "#
    );
    toml::from_str(&text).expect("an hls output")
}

#[tokio::test(flavor = "multi_thread")]
async fn an_hls_ladder_is_packaged_from_the_planners_encoders() {
    let mut mix = mixer(config()).await;
    mix.add_output(&output("steady", Some(want(144, 400)))).unwrap();
    mix.add_output(&hls_output("ladder-in-mixer")).expect("an hls output needs no uri");
    assert_eq!(encoders(&mix).len(), 2, "180p, and one 144p for both outputs: {:?}", encoders(&mix));
    let plan = mix.renditions().shared_view().read().clone();
    let shared = plan.nodes.iter().filter(|n| n.kind == "encode").find(|n| n.serves.len() == 2);
    assert!(shared.is_some(), "the 144p encode serves both: {plan:?}");

    let s = stream::get("ladder-in-mixer").expect("the stream is published");
    let started = Instant::now();
    let whole = |id: &str| s.track(id).map_or(0, |t| t.view().segments.iter().filter(|x| x.complete).count());
    while ["180p", "144p", "audio"].iter().any(|id| whole(id) < 3) && started.elapsed() < Duration::from_secs(20) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let ids: Vec<(String, TrackKind)> = s.tracks().iter().map(|t| (t.id.clone(), t.kind)).collect();
    assert!(["180p", "144p", "audio"].iter().all(|id| whole(id) >= 3), "every rung has segments: {}", s.status());
    assert_eq!(ids.len(), 3, "two rungs and the sound, nothing named programme: {ids:?}");
    let numbers = |id: &str| -> Vec<u64> { s.track(id).unwrap().view().segments.iter().filter(|x| x.complete).map(|x| x.msn).collect() };
    let (top, low) = (numbers("180p"), numbers("144p"));
    assert!(top.iter().any(|m| low.contains(m)), "the rungs number the same seconds alike: {top:?} {low:?}");
    let info = s.track("144p").unwrap().info();
    assert_eq!((info.height, info.declared_kbps), (144, 400));

    mix.remove_output(&"ladder-in-mixer".to_string()).unwrap();
    assert!(stream::get("ladder-in-mixer").is_none(), "the stream goes with the output");
    assert_eq!(encoders(&mix).len(), 1, "the 144p stays for steady, the 180p goes: {:?}", encoders(&mix));
    mix.shutdown();
}
