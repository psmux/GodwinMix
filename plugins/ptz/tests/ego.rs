//! The follow loop tells the tracker how the camera moves: a camera's
//! optics come from its settings, and a joystick pan while the loop looks
//! shows in `track_state` as the picture's own motion.

mod common;

use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use gmx_ptz::core::Core;
use gmx_ptz::rig::Rig;
use gmx_ptz::tools::{call, Shared};
use gmx_ptz::{CameraSpec, Settings};
use serde_json::json;

#[test]
fn optics_are_read_with_defaults_and_ranges() {
    let spec = CameraSpec::from_value(&json!({"id": "cam-1", "protocol": "fake", "optics": {"hfov_deg": 50, "pan_deg_s": 9000}})).unwrap();
    assert_eq!((spec.optics.hfov_deg, spec.optics.pan_deg_s, spec.optics.vfov_deg), (50.0, 720.0, 17.0));
    assert_eq!(spec.public()["optics"]["hfov_deg"], 50.0);
}

#[test]
fn a_joystick_pan_shows_as_the_picture_moving_not_as_people_walking() {
    let dir = std::env::temp_dir().join(format!("gmx-ptz-ego-{}", std::process::id()));
    let settings = Settings::from_value(&json!({"data_dir": dir, "detector": "motion"}));
    let rig: Shared = Arc::new(Rig::new(settings, Core::from_url(&common::serve(), None)).into());
    let optics = json!({"hfov_deg": 60, "pan_deg_s": 30});
    call(&rig, "camera_add", &json!({"id": "cam-pulpit", "protocol": "fake", "source": "pulpit", "optics": optics})).unwrap();
    call(&rig, "track_observe", &json!({"wait_ms": 2000})).unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    let mut dx = 0.0;
    while Instant::now() < until && dx > -0.4 {
        call(&rig, "move", &json!({"pan": 1.0})).unwrap();
        sleep(Duration::from_millis(100));
        let state = call(&rig, "track_state", &json!({})).unwrap().structured_content.unwrap();
        dx = state["camera_motion"]["dx"].as_f64().unwrap_or(0.0);
    }
    assert!((dx + 0.5).abs() < 0.01, "a full speed pan right at 30 of 60 degrees a second moves the picture left half a frame a second: {dx}");
    call(&rig, "stop", &json!({})).unwrap();
}
