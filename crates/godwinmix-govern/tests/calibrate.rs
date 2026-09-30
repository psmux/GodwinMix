//! Calibration end to end on this machine: the real catalogue, the real
//! encoders, the file written and read back, and a governor admitting
//! against what was measured.
//!
//! One test in this binary on purpose: calibration reads the CPU time of the
//! whole process, and a second test running beside it would be counted.
//!
//! A machine with no hardware encoder (a CI runner) runs the software half
//! and skips the hardware assertions. `GMX_SKIP_HARDWARE=1` skips them on a
//! machine that has one.

use godwinmix_govern::calibrate::{calibrate, fingerprint_for, Options};
use godwinmix_govern::store::{decide, Decision, Store};
use godwinmix_govern::{Governor, GovernorConfig, Kind, Profile};
use godwinmix_protocol::rendition::{Fps, VideoCodec, VideoShape};
use std::time::Instant;

#[test]
fn this_machine_is_measured_stored_and_governed() {
    gstreamer::init().unwrap();
    let cat = godwinmix_core::catalogue::Catalogue::shipped().unwrap();
    // The station's own list: the catalogue as the core reads it.
    let cands = godwinmix_core::render::candidates::candidates(&cat, godwinmix_core::config::Accel::Auto);
    let audio = godwinmix_core::render::candidates::audio(&cat);
    if cands.is_empty() {
        eprintln!("no video encoder from the catalogue is installed here; nothing to calibrate");
        return;
    }
    let t0 = Instant::now();
    let cal = calibrate(&cands, &audio, &Options::default());
    let took = t0.elapsed();
    eprintln!("calibration took {:.2} s on {} ({} cores)", took.as_secs_f64(), cal.machine.cpu, cal.machine.cores);
    for e in &cal.encoders {
        let pts: Vec<String> = e.points.iter().map(|p| format!("{}p {} mc {} ms", p.height, p.cpu_millicores, p.wall_ms)).collect();
        let pre: Vec<String> = e.presets.iter().map(|p| format!("{} {}", p.preset, p.point.cpu_millicores)).collect();
        eprintln!("  {:<24} {:<14} {} | presets {} | sessions {:?}", e.slot.id, e.element, pts.join(", "), pre.join(", "), e.sessions);
    }
    eprintln!("  scale {:?} mc per Mpix/s, decoders {:?}", cal.scale_per_mpix, cal.decoders);
    eprintln!("  audio {:?}", cal.audio.iter().map(|a| (&a.id, a.cpu_millicores)).collect::<Vec<_>>());
    for n in &cal.notes {
        eprintln!("  note: {n}");
    }
    assert!(took.as_secs() < 20, "calibration must stay short; took {took:?}");
    assert_eq!(cal.fingerprint, fingerprint_for(&cands), "the key is stable");

    let software: Vec<_> = cal.encoders.iter().filter(|e| !e.slot.hardware).collect();
    assert!(!software.is_empty(), "at least one software encoder measured: {:?}", cal.notes);
    for e in &software {
        assert_eq!(e.points.len(), 2);
        assert!(e.points[1].cpu_millicores > e.points[0].cpu_millicores, "1080p costs more than 720p: {e:?}");
    }

    let dir = std::env::temp_dir().join(format!("gmx-govern-e2e-{}", std::process::id()));
    let store = Store::new(&dir);
    store.save(&cal).unwrap();
    // Compared by field: a float written as JSON may come back one bit off.
    let Decision::Use(back) = decide(&store, &cal.fingerprint, false, false) else { panic!("the stored file is used") };
    assert_eq!((&back.fingerprint, &back.encoders, &back.audio), (&cal.fingerprint, &cal.encoders, &cal.audio));
    let _ = std::fs::remove_dir_all(&dir);

    let profile = Profile::from_calibration(cal.clone());
    let g = Governor::new(GovernorConfig::default(), profile.clone());
    let shape = VideoShape { codec: VideoCodec::H264, width: 1280, height: 720, fps: Fps::whole(30), bitrate_kbps: 3000, keyframe_ms: 2000 };
    let slot = profile.encoders(VideoCodec::H264).into_iter().next().expect("an H.264 encoder");
    let t = g.admit_encode(&slot, &shape, "a 720p30 H.264 rendition", Kind::Rung { index: 1 }).granted();
    assert!(t.is_some(), "one 720p30 encode fits on a machine at rest");

    if std::env::var_os("GMX_SKIP_HARDWARE").is_some() {
        return;
    }
    for e in cal.encoders.iter().filter(|e| e.slot.hardware) {
        let s = e.sessions.expect("a hardware encoder has its sessions probed");
        assert!(s.opened >= 1, "{e:?}");
        let c = profile.encode_cost(&e.slot, &shape);
        assert!(c.device_sessions == 1 && c.device_millis > 0, "{c:?}");
    }
}
