//! A made up machine for the unit tests: eight cores, x264 on the CPU and
//! one H.264 hardware encoder that refuses a fourth session. The numbers
//! are round so a test can say what it expects.

use crate::calibration::{Calibration, EncoderCal, Point, PresetPoint, Sessions, FORMAT};
use crate::profile::Profile;
use godwinmix_protocol::rendition::{EncoderSlot, Fps, VideoCodec, VideoShape};

pub fn x264() -> EncoderSlot {
    EncoderSlot { id: "h264-software-x264".into(), codec: VideoCodec::H264, hardware: false, device: None }
}

pub fn gpu() -> EncoderSlot {
    EncoderSlot { id: "h264-gpu".into(), codec: VideoCodec::H264, hardware: true, device: Some("gpu".into()) }
}

pub fn shape(w: u32, h: u32, fps: u32) -> VideoShape {
    VideoShape { codec: VideoCodec::H264, width: w, height: h, fps: Fps::whole(fps), bitrate_kbps: 0, keyframe_ms: 0 }
}

fn point(w: u32, h: u32, cpu: u32, wall: u32) -> Point {
    Point { width: w, height: h, fps: Fps::whole(30), cpu_millicores: cpu, wall_ms: wall }
}

pub fn calibration() -> Calibration {
    let top = point(1920, 1080, 2000, 500);
    let preset = |name: &str, cpu: u32| PresetPoint { preset: name.into(), point: Point { cpu_millicores: cpu, ..top.clone() } };
    Calibration {
        format: FORMAT,
        fingerprint: "test".into(),
        encoders: vec![
            EncoderCal {
                slot: x264(),
                element: "x264enc".into(),
                preset: Some("veryfast".into()),
                // Proportional: 1000 at 720p30, 2250 at 1080p30.
                points: vec![point(1280, 720, 889, 300), top.clone()],
                presets: vec![preset("ultrafast", 1000), preset("superfast", 1400), preset("veryfast", 2000), preset("faster", 3000)],
                sessions: None,
            },
            EncoderCal {
                slot: gpu(),
                element: "gpuh264enc".into(),
                preset: None,
                points: vec![point(1280, 720, 40, 100), point(1920, 1080, 60, 200)],
                presets: Vec::new(),
                sessions: Some(Sessions { opened: 3, refused: true }),
            },
        ],
        scale_per_mpix: Some(2.0),
        ..Default::default()
    }
}

pub fn profile() -> Profile {
    Profile::from_calibration(calibration())
}
