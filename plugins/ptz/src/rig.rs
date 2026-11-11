//! Every camera this plugin drives: its description, the thread that owns
//! its driver, its presets' pictures and its follow loop when one runs.
//!
//! Cameras come from two places. The settings' `cameras` list is the core's
//! to change, with `configure`. Cameras added with `camera_add` are kept in
//! `<data_dir>/cameras.json` so they come back after a restart.

use std::collections::BTreeMap;

use godwinmix_ptz::{open, Capabilities};
use serde_json::Value;

use crate::core::Core;
use crate::follow::Loop;
use crate::presets::Store;
use crate::settings::{CameraSpec, Settings};
use crate::worker::Handle;

pub struct Camera {
    pub spec: CameraSpec,
    pub handle: Handle,
    pub presets: Store,
    pub caps: Capabilities,
    pub follow: Option<Loop>,
    /// Added with `camera_add` rather than from the settings.
    pub added: bool,
}

pub struct Rig {
    pub cameras: BTreeMap<String, Camera>,
    pub settings: Settings,
    pub core: Option<Core>,
    pub problems: Vec<String>,
}

impl Rig {
    pub fn new(settings: Settings, core: Option<Core>) -> Rig {
        let empty = Settings { cameras: Vec::new(), ..settings.clone() };
        let mut rig = Rig { cameras: BTreeMap::new(), settings: empty, core, problems: Vec::new() };
        for spec in rig.load_added() {
            if let Err(why) = rig.add(spec, true) {
                rig.problems.push(why);
            }
        }
        rig.configure(settings);
        rig
    }

    /// Apply new settings: cameras that left the list go, new and changed
    /// ones are opened again. Cameras added by tool stay as they are.
    pub fn configure(&mut self, settings: Settings) {
        let gone: Vec<String> = self.cameras.iter().filter(|(id, c)| !c.added && !settings.cameras.iter().any(|s| &s.id == *id && *s == c.spec)).map(|(id, _)| id.clone()).collect();
        for id in gone {
            self.cameras.remove(&id);
        }
        self.problems = settings.problems.clone();
        for spec in settings.cameras.clone() {
            if !self.cameras.contains_key(&spec.id) {
                if let Err(why) = self.add(spec, false) {
                    self.problems.push(why);
                }
            }
        }
        self.settings = settings;
    }

    /// Open a driver for `spec` and start its thread. ONVIF asks the camera
    /// where its services are, so an ONVIF camera that is not there fails
    /// here; the others connect on the first command.
    pub fn add(&mut self, spec: CameraSpec, added: bool) -> Result<&Camera, String> {
        let driver = open(&spec.target()).map_err(|e| format!("camera '{}': {}", spec.id, e.message))?;
        let caps = driver.capabilities();
        let handle = Handle::spawn(&spec.id, driver, self.settings.joystick_hold);
        let presets = Store::new(&self.settings.data_dir, &spec.id);
        let id = spec.id.clone();
        self.cameras.insert(id.clone(), Camera { spec, handle, presets, caps, follow: None, added });
        if added {
            self.save_added();
        }
        Ok(&self.cameras[&id])
    }

    /// Remove a camera. Its thread stops it on the way out. Answers whether
    /// it was there.
    pub fn remove(&mut self, id: &str) -> bool {
        let was = self.cameras.remove(id).map(|c| c.added);
        if was == Some(true) {
            self.save_added();
        }
        was.is_some()
    }

    pub fn ids(&self) -> Vec<&str> {
        self.cameras.keys().map(String::as_str).collect()
    }

    fn added_file(&self) -> std::path::PathBuf {
        self.settings.data_dir.join("cameras.json")
    }

    fn load_added(&self) -> Vec<CameraSpec> {
        let text = std::fs::read_to_string(self.added_file()).unwrap_or_default();
        let list: Vec<Value> = serde_json::from_str(&text).unwrap_or_default();
        list.iter().filter_map(|v| CameraSpec::from_value(v).ok()).collect()
    }

    /// The cameras added by tool, with their passwords: the file is the
    /// plugin's own, under the data directory.
    fn save_added(&self) {
        let list: Vec<Value> = self
            .cameras
            .values()
            .filter(|c| c.added)
            .map(|c| {
                let mut v = c.spec.public();
                v["password"] = Value::String(c.spec.password.clone());
                v
            })
            .collect();
        let _ = std::fs::create_dir_all(&self.settings.data_dir);
        let _ = std::fs::write(self.added_file(), serde_json::to_string_pretty(&list).unwrap_or_default());
    }
}
