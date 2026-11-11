//! Tool calls on one camera at the same time: a `preset_save` the camera
//! takes 1.5 s over never holds back the joystick, and on a camera with one
//! connection the watchdog stops it as soon as the slow command is done.

use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use godwinmix_ptz::fake::{self, FakePtz};
use godwinmix_ptz::{Capabilities, Exposure, Focus, Position, Ptz, Result as PtzResult, Velocity};
use gmx_ptz::rig::{Camera, Rig};
use gmx_ptz::settings::CameraSpec;
use gmx_ptz::tools::{call_later, Outcome, Shared};
use gmx_ptz::worker::Handle;
use gmx_ptz::Settings;
use serde_json::json;

fn rig(name: &str, driver: Box<dyn Ptz>) -> Shared {
    let dir = std::env::temp_dir().join(format!("gmx-ptz-concurrent-{name}-{}", std::process::id()));
    let settings = Settings::from_value(&json!({"data_dir": dir}));
    let spec = CameraSpec::from_value(&json!({"id": "cam-pulpit", "protocol": "fake", "address": "fake"})).unwrap();
    let handle = Handle::spawn("cam-pulpit", driver, settings.joystick_hold);
    let cam = Camera { spec, handle, presets: gmx_ptz::presets::Store::new(&dir, "cam-pulpit"), caps: Capabilities { absolute: true, position: true, focus: true, exposure: true, presets: 100 }, follow: None, added: false };
    let mut rig = Rig::new(settings, None);
    rig.insert(cam);
    Arc::new(rig.into())
}

fn start(rig: &Shared, tool: &str, a: serde_json::Value) -> Receiver<Outcome> {
    let (tx, rx) = channel();
    call_later(rig, tool, &a, move |o| {
        let _ = tx.send(o);
    });
    rx
}

#[test]
fn moves_keep_the_camera_moving_while_a_preset_is_saved_slowly() {
    let head = fake::shared();
    head.lock().unwrap().save_delay = Duration::from_millis(1500);
    let rig = rig("two-lanes", Box::new(FakePtz(head.clone())));
    assert_eq!(rig.lock().unwrap().cameras["cam-pulpit"].handle.lanes, 2);
    let saving = start(&rig, "preset_save", json!({"preset": 3, "name": "Pulpit"}));
    let began = Instant::now();
    while began.elapsed() < Duration::from_millis(1200) {
        let sent = Instant::now();
        let moved = start(&rig, "move", json!({"pan": 0.8})).recv_timeout(Duration::from_millis(50));
        assert!(moved.is_ok_and(|o| o.is_ok()), "a move was held up {} ms into the save", began.elapsed().as_millis());
        assert!(sent.elapsed() < Duration::from_millis(50));
        sleep(Duration::from_millis(20));
        assert_eq!(head.lock().unwrap().velocity.pan, 0.8, "the camera stopped {} ms into the save", began.elapsed().as_millis());
        sleep(Duration::from_millis(80));
    }
    assert!(saving.try_recv().is_err(), "the save is still on the camera");
    let stop = Instant::now();
    start(&rig, "stop", json!({})).recv_timeout(Duration::from_millis(200)).expect("stop answers during the save").expect("stopped");
    assert!(stop.elapsed() < Duration::from_millis(200));
    assert_eq!(head.lock().unwrap().velocity, Velocity::STOP);
    saving.recv_timeout(Duration::from_secs(3)).expect("the save answers").expect("saved");
    assert!(head.lock().unwrap().presets.contains_key(&3));
    let status = rig.lock().unwrap().cameras["cam-pulpit"].handle.status.lock().unwrap().clone();
    assert_eq!(status.watchdog_stops, 0, "no move ran out while the save was in flight");
}

/// A camera that takes one connection only, as VISCA does.
struct OneLink(FakePtz, Duration);

impl Ptz for OneLink {
    fn protocol(&self) -> &'static str {
        "fake"
    }
    fn capabilities(&self) -> Capabilities {
        self.0.capabilities()
    }
    fn move_at(&mut self, v: Velocity) -> PtzResult<()> {
        self.0.move_at(v)
    }
    fn zoom(&mut self, s: f32) -> PtzResult<()> {
        self.0.zoom(s)
    }
    fn focus(&mut self, f: Focus) -> PtzResult<()> {
        self.0.focus(f)
    }
    fn goto_preset(&mut self, n: u16) -> PtzResult<()> {
        self.0.goto_preset(n)
    }
    fn save_preset(&mut self, n: u16) -> PtzResult<()> {
        sleep(self.1);
        self.0.save_preset(n)
    }
    fn position(&mut self) -> PtzResult<Position> {
        self.0.position()
    }
    fn goto(&mut self, p: Position) -> PtzResult<()> {
        self.0.goto(p)
    }
    fn exposure(&mut self, e: Exposure) -> PtzResult<()> {
        self.0.exposure(e)
    }
}

#[test]
fn on_one_connection_the_watchdog_stops_the_camera_as_soon_as_a_slow_command_ends() {
    let head = fake::shared();
    let rig = rig("one-lane", Box::new(OneLink(FakePtz(head.clone()), Duration::from_millis(800))));
    assert_eq!(rig.lock().unwrap().cameras["cam-pulpit"].handle.lanes, 1);
    start(&rig, "move", json!({"pan": 1.0})).recv().unwrap().unwrap();
    sleep(Duration::from_millis(30));
    let saving = start(&rig, "preset_save", json!({"preset": 1}));
    // The tool call answers at once even though the camera is busy.
    start(&rig, "camera_list", json!({})).recv_timeout(Duration::from_millis(50)).expect("camera_list during the save").unwrap();
    saving.recv_timeout(Duration::from_secs(3)).expect("the save answers").expect("saved");
    sleep(Duration::from_millis(40));
    assert_eq!(head.lock().unwrap().velocity, Velocity::STOP, "the hold ran out during the save, so the camera stops right after it");
    let status = rig.lock().unwrap().cameras["cam-pulpit"].handle.status.lock().unwrap().clone();
    assert_eq!(status.watchdog_stops, 1);
}
