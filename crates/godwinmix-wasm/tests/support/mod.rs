//! A test core with the `wasm-ease` component installed as its transition.
//!
//! Shared by the two frame interval tests, which live in files of their own so
//! that each gets a process of its own. Two mixers in one process tear down
//! over each other's GStreamer elements, and what that costs is a confusing
//! crash at the end of a run rather than a result.

use godwinmix_core::mixer::{transition, Mixer, MixerHandle, ProgramScene};
use godwinmix_core::plugin::supervisor::Supervisor;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// One frame at 30 fps plus the margin Phase 6 states its hook criteria with.
pub const MAX_FRAME_INTERVAL: Duration = Duration::from_millis(34);

pub fn ease() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/wasm-ease")
}

#[allow(dead_code)]
/// `[plugins.wasm-ease] wasm_fuel = <n>`, for a test that wants a component
/// cut off in the middle of a call.
pub fn fuel(n: u64) -> BTreeMap<String, toml::Table> {
    let table: toml::Table = toml::from_str(&format!("wasm_fuel = {n}")).expect("a table");
    BTreeMap::from([("wasm-ease".to_string(), table)])
}

/// Install the component into a plugins directory of this test's own.
pub fn install(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-wasm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a plugins directory");
    godwinmix_core::plugin::loader::set_dir(dir.clone());
    godwinmix_core::plugin::loader::set_runtime_dir(dir.join("run"));
    godwinmix_core::plugin::loader::install_from_path(&ease()).expect("installing wasm-ease");
    dir
}

pub fn clean(dir: PathBuf) {
    let _ = godwinmix_core::plugin::loader::uninstall("wasm-ease");
    let _ = std::fs::remove_dir_all(dir);
}

/// A running mixer with two live sources and the component as its renderer.
pub struct Core {
    pub mixer: Mixer,
    pub supervisor: Arc<Supervisor>,
    handle: MixerHandle,
}

impl Core {
    pub async fn start(settings: BTreeMap<String, toml::Table>) -> Core {
        let cfg: godwinmix_core::config::Config = toml::from_str(
            "[canvas]\nwidth = 320\nheight = 180\nfps = 30\nsample_rate = 48000\n\
             channels = 2\n\n[multiview]\nenabled = false\n",
        )
        .expect("a config");
        let (mut mixer, handle, _commands, _bus) =
            Mixer::build(cfg).expect("the mixer builds");
        mixer.start().expect("the programme starts");

        let supervisor = Supervisor::new(mixer.canvas().clone(), settings);
        for (provide, why) in supervisor.start_all() {
            panic!("`{provide}` would not start: {why}");
        }
        assert_eq!(
            supervisor.transition_names(),
            vec!["wasm-ease".to_string()],
            "the component should be a transition this core can name"
        );
        mixer.set_transition_renderer(supervisor.clone());

        for id in ["cam1", "cam2"] {
            let cfg: godwinmix_core::config::SourceConfig =
                toml::from_str(&format!("id = \"{id}\"\nuri = \"test://smpte\"\n"))
                    .expect("a source");
            mixer.add_source(&cfg, None).expect("adding a source");
        }
        for _ in 0..100 {
            let live = mixer
                .status()
                .sources
                .iter()
                .filter(|s| s.state == godwinmix_core::state::SourceState::Live)
                .count();
            if live == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Core { mixer, supervisor, handle }
    }

    pub fn scene(&self, id: &str) -> ProgramScene {
        let canvas = self.mixer.canvas().clone();
        ProgramScene {
            name: id.to_string(),
            placements: vec![godwinmix_core::mixer::slots::Placement::full_canvas(
                id.to_string(),
                &canvas,
            )],
        }
    }

    /// Take over with the component's transition.
    pub fn ease_to(&mut self, id: &str, duration_ms: u64) -> anyhow::Result<()> {
        let scene = self.scene(id);
        self.mixer.take_scene_over(
            scene,
            None,
            None,
            Some(transition::TransitionSpec {
                kind: transition::Kind::Plugin("wasm-ease".into()),
                duration_ms,
            }),
        )
    }

    pub fn shutdown(self) {
        let Core { mut mixer, supervisor, handle } = self;
        supervisor.shutdown();
        mixer.shutdown();
        drop(handle);
    }
}
