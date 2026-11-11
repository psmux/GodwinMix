//! What one frame of the follow loop costs, and one camera command.
//!
//!   cargo bench -p gmx-ptz
//!
//! * decode: one 320 by 180 JPEG from the core's MJPEG stream to RGB.
//! * step: the tracking engine's Follower on that picture.
//! * visca: one pan and tilt command and its acknowledgement, over TCP on
//!   loopback, to the fake camera.
//!
//! No criterion: the numbers are medians of plain timings, printed one per
//! line, so a person compares them by eye and CI can grep them.

#[path = "../tests/common/mod.rs"]
mod common;

use std::time::{Duration, Instant};

use godwinmix_ptz::{fake, visca, Ptz, Velocity};
use gmx_ptz::follow::detector::{motion, Kind, Switching};
use gmx_ptz::follow::run_follower;
use gmx_ptz::frames::mjpeg::decode;

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn main() {
    let jpegs: Vec<Vec<u8>> = (0..120).map(common::picture).collect();
    let decode_times: Vec<Duration> = jpegs
        .iter()
        .map(|j| {
            let t = Instant::now();
            std::hint::black_box(decode(j).unwrap());
            t.elapsed()
        })
        .collect();
    let pictures: Vec<_> = jpegs.iter().map(|j| decode(j).unwrap()).collect();
    let mut f = run_follower(godwinmix_track::Framing::Waist, Switching::ready(Kind::Motion, motion()));
    let mut out = godwinmix_track::FollowOutput::default();
    let start = Instant::now();
    let mut step_times = Vec::new();
    for (i, p) in pictures.iter().cycle().take(600).enumerate() {
        let frame = p.frame().unwrap();
        let t = Instant::now();
        f.step_into(&frame, start + Duration::from_millis(66 * i as u64), &mut out);
        step_times.push(t.elapsed());
    }
    let head = fake::shared();
    let at = fake::visca::serve_tcp(head);
    let mut cam = visca::Visca::over_tcp(at, visca::Ranges::default(), Duration::from_millis(500));
    let mut cmd_times = Vec::new();
    for i in 0..400 {
        let t = Instant::now();
        cam.move_at(Velocity::new(if i % 2 == 0 { 0.3 } else { -0.3 }, 0.0, 0.0)).unwrap();
        cmd_times.push(t.elapsed());
    }
    let d = median(decode_times);
    let s = median(step_times);
    println!("decode 320x180 jpeg: {:.2} ms median", d.as_secs_f64() * 1000.0);
    println!("follower step 320x180: {:.2} ms median", s.as_secs_f64() * 1000.0);
    println!("frame total: {:.2} ms, {:.0} frames a second on one core", (d + s).as_secs_f64() * 1000.0, 1.0 / (d + s).as_secs_f64());
    println!("visca command over tcp loopback: {:.3} ms median", median(cmd_times).as_secs_f64() * 1000.0);
}
