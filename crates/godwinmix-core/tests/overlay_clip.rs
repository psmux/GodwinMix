//! A clip with an alpha channel on the programme: drawn over the picture
//! under it, played in real time, and an opaque clip left as it always was.

use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::slots::{Placement, Sizing};
use godwinmix_core::mixer::{self, Command, Mixer, MixerHandle, ProgramScene};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "overlay/support.rs"]
mod support;
use support::*;

const BLUE: (u8, u8, u8) = (32, 240, 118);
const RED: (u8, u8, u8) = (63, 102, 240);

/// Five seconds of a clip whose left half is red and right half clear, as
/// PNG frames in a QuickTime file: alpha video made with GStreamer alone.
fn half_red_clip(path: &std::path::Path, alpha: bool) -> bool {
    gst::init().unwrap();
    let desc = format!(
        "videotestsrc num-buffers=50 pattern=red ! video/x-raw,format=AYUV,width=32,height=36,framerate=10/1 \
         ! videobox right=-32 border-alpha=0 ! videoconvert ! video/x-raw,format={} ! pngenc snapshot=false \
         ! qtmux ! filesink location=\"{}\"",
        if alpha { "RGBA" } else { "RGB" },
        path.display().to_string().replace('\\', "/")
    );
    let Ok(p) = gst::parse::launch(&desc) else { return false };
    p.set_state(gst::State::Playing).ok();
    let done = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Eos, gst::MessageType::Error]);
    p.set_state(gst::State::Null).ok();
    done.is_some_and(|m| m.type_() == gst::MessageType::Eos)
}

/// Which formats keep their alpha on this machine. Run by hand with
/// `GMX_ALPHA_MEDIA` naming a folder of clips whose left half is opaque red
/// and right half clear (made with ffmpeg in VP8 and VP9 WebM, ProRes 4444,
/// QuickTime Animation and HEVC with alpha); prints one line per clip.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn which_alpha_formats_this_machine_keeps() {
    let Ok(dir) = std::env::var("GMX_ALPHA_MEDIA") else { return };
    let mut clips: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect();
    clips.retain(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("webm" | "mov")));
    clips.sort();
    for clip in clips {
        let (handle, frames, thread) = running();
        add(&handle, SourceConfig::bare("bg", "test://blue")).await;
        add(&handle, SourceConfig::bare("clip", &clip.to_string_lossy())).await;
        take(&handle, vec![full("bg"), at("clip", 0, 0, 128, 72)]).await;
        settle(1_500).await;
        let f = frames.latest().unwrap();
        let (opaque, clear) = (f.yuv(20, 30), f.yuv(100, 30));
        stop(handle, thread);
        let kept = near(opaque, RED) && near(clear, BLUE);
        println!("{:<20} {} (opaque half {opaque:?}, clear half {clear:?})", clip.file_name().unwrap().to_string_lossy(), if kept { "keeps alpha" } else { "flat" });
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_clip_with_alpha_shows_the_picture_under_its_clear_half() {
    let dir = scratch("clip");
    let clip = dir.join("sting.mov");
    if !half_red_clip(&clip, true) {
        println!("skipping: this GStreamer cannot write PNG in QuickTime");
        return;
    }
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("bg", "test://blue")).await;
    let mut events = handle.subscribe();
    add(&handle, SourceConfig::bare("sting", &clip.to_string_lossy())).await;
    take(&handle, vec![full("bg"), at("sting", 0, 0, 128, 72)]).await;
    settle(2_500).await;
    let f = frames.latest().expect("programme frames");
    let (opaque, clear) = (f.yuv(20, 30), f.yuv(100, 30));
    let worst = frames.worst_interval();
    let mut ended = 0;
    while let Ok(e) = events.try_recv() {
        ended += matches!(&e.event, godwinmix_core::state::Event::SourceStateChanged { source, .. } if source == "sting") as usize;
    }
    stop(handle, thread);
    // Five seconds of clip. Unpaced it would decode in milliseconds, reach
    // its end and be restarted over and over, each time saying so.
    assert!(ended <= 2, "the clip played in real time rather than racing to its end: {ended} state changes in 2.5 s");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(near(opaque, RED), "the clip's opaque half is red: {opaque:?}");
    assert!(near(clear, BLUE), "its clear half shows the blue under it: {clear:?}");
    assert!(worst < 34.0 * 3.0 * godwinmix_core::plugin::harness::timing_slack(), "the programme kept its rate: {worst:.1} ms");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_same_clip_without_alpha_goes_through_the_compositor_as_before() {
    let dir = scratch("flat");
    let clip = dir.join("flat.mov");
    if !half_red_clip(&clip, false) {
        println!("skipping: this GStreamer cannot write PNG in QuickTime");
        return;
    }
    let (handle, frames, thread) = running();
    add(&handle, SourceConfig::bare("bg", "test://blue")).await;
    add(&handle, SourceConfig::bare("flat", &clip.to_string_lossy())).await;
    take(&handle, vec![full("bg"), at("flat", 0, 0, 128, 72)]).await;
    settle(1_500).await;
    let f = frames.latest().expect("programme frames");
    let (red, black) = (f.yuv(20, 30), f.yuv(100, 30));
    stop(handle, thread);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(near(red, RED), "{red:?}");
    assert!(near(black, (16, 128, 128)), "an opaque clip covers what is under it: {black:?}");
}
