//! The example transition plugin, against the harness's transition checks.
//!
//! `plugins/wipe` is the worked example of the fourth plugin kind, and this is
//! what says it works: the manifest is valid, the process starts and shakes
//! hands, and `render` answers the contract at the start, the middle and the
//! end of a crossing. Nothing here knows what a wipe looks like; what is
//! checked is what any transition plugin has to do, so a dissolve or a clock
//! wipe written by somebody else passes the same test.

// Unix only, and the whole file rather than each test: the supervisor starts
// the plugin as a real child process and the fixtures around it are shell.
// `plugins/wipe`'s own unit tests are not gated and do run on Windows, so what
// is missing there is the supervisor driving it end to end, not the wipe.
// docs/explanation/cross-platform.md lists this with the rest.
#![cfg(unix)]

use godwinmix_core::plugin::harness;
use godwinmix_core::plugin::supervisor::Supervisor;
use std::path::PathBuf;
use std::time::Duration;

/// The plugin's directory in the checkout.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/wipe")
}

/// Put the built binary where the manifest says it is.
///
/// A plugin under `plugins/` is a workspace member, so cargo builds it into
/// the workspace's own target directory, while its manifest names a path
/// inside the plugin (`bin/gmx-wipe`) because that is what an installed copy
/// looks like. The copy is one line here and one line in the plugin's README.
fn built() -> Option<PathBuf> {
    let name = "gmx-wipe";
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let built = ["release", "debug"]
        .iter()
        .map(|profile| workspace.join(profile).join(name))
        .find(|p| p.is_file())?;
    let bin = root().join("bin");
    std::fs::create_dir_all(&bin).ok()?;
    let at = bin.join(name);
    std::fs::copy(&built, &at).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o755)).ok()?;
    }
    Some(at)
}

#[test]
fn the_wipe_plugin_passes_the_transition_checks() {
    let manifest = harness::check_manifest(&root());
    assert!(manifest.passed, "{}: {}", manifest.name, manifest.detail);

    let Some(_binary) = built() else {
        // `cargo test -p godwinmix-core` on its own does not build the plugin.
        // Saying so beats a failure that looks like the plugin is broken.
        println!(
            "gmx-wipe is not built, so the transition checks were skipped. Build it with \
             `cargo build -p gmx-wipe` and run this again."
        );
        return;
    };

    let spawn = harness::check_spawn(&root(), "wipe");
    assert!(spawn.passed, "{}: {}", spawn.name, spawn.detail);
    let configure = harness::check_configure(&root(), "wipe");
    assert!(configure.passed, "{}: {}", configure.name, configure.detail);

    let transition = harness::check_transition(&root(), "wipe");
    println!("{}: {}", transition.name, transition.detail);
    assert!(transition.passed, "{}: {}", transition.name, transition.detail);
}

/// The whole chain, end to end: a take naming a plugin transition reaches the
/// plugin, and what it answers is on the compositor's pads.
///
/// This is the one that would have caught every seam between the take and the
/// process: the method table accepting the name, the supervisor finding the
/// instance, the mixer sampling it inside its budget, and the answers becoming
/// control bindings rather than being dropped.
#[test]
fn a_take_naming_a_plugin_transition_drives_the_pads() {
    let Some(_binary) = built() else {
        println!("gmx-wipe is not built, so the end to end take was skipped");
        return;
    };
    let _ = gstreamer::init();
    let dir = std::env::temp_dir().join(format!("gmx-wipe-take-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a plugins directory");
    godwinmix_core::plugin::loader::set_dir(dir.clone());
    godwinmix_core::plugin::loader::set_runtime_dir(dir.join("run"));
    godwinmix_core::plugin::loader::install_from_path(&root()).expect("installing the wipe");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let cfg: godwinmix_core::config::Config = toml::from_str(
            "[canvas]\nwidth = 320\nheight = 180\nfps = 30\nsample_rate = 48000\nchannels = 2\n\n[multiview]\nenabled = false\n",
        )
        .expect("a config");
        let (mut mixer, handle, _commands, _bus) =
            godwinmix_core::mixer::Mixer::build(cfg).expect("the mixer builds");
        mixer.start().expect("the programme starts");

        let supervisor = Supervisor::new(mixer.canvas().clone(), Default::default());
        supervisor.start_all();
        assert_eq!(
            supervisor.transition_names(),
            vec!["wipe".to_string()],
            "the wipe should be a transition this core can name"
        );
        mixer.set_transition_renderer(supervisor.clone());

        for id in ["cam1", "cam2"] {
            let cfg: godwinmix_core::config::SourceConfig =
                toml::from_str(&format!("id = \"{id}\"\nuri = \"test://smpte\"\n"))
                    .expect("a source");
            mixer.add_source(&cfg, None).expect("adding a source");
        }
        // Long enough for both to be judged live, which is what puts them on
        // the canvas at all.
        for _ in 0..100 {
            if mixer.status().sources.iter().filter(|s| s.state == godwinmix_core::state::SourceState::Live).count() == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let canvas = mixer.canvas().clone();
        let one = |id: &str| godwinmix_core::mixer::ProgramScene {
            name: id.to_string(),
            placements: vec![godwinmix_core::mixer::slots::Placement::full_canvas(
                id.to_string(),
                &canvas,
            )],
        };
        mixer.take_scene(one("cam1"), None).expect("the first scene");
        mixer
            .take_scene_over(
                one("cam2"),
                None,
                None,
                Some(godwinmix_core::mixer::transition::TransitionSpec {
                    kind: godwinmix_core::mixer::transition::Kind::Plugin("wipe".into()),
                    duration_ms: 400,
                }),
            )
            .expect("a wipe");

        // The proof: the incoming scene's pad is being driven on `xpos`, which
        // is what a wipe moves and what no built in transition touches.
        let driven = mixer.pool_for_tests().driven_by_a_transition("xpos");
        assert!(
            driven.contains(&"cam2".to_string()),
            "the plugin's curves never reached the incoming pad; driven: {driven:?}"
        );
        assert!(mixer.transition_window().is_some(), "the transition should be on the canvas");

        supervisor.shutdown();
        mixer.shutdown();
        drop(handle);
    });
    let _ = godwinmix_core::plugin::loader::uninstall("wipe");
    let _ = std::fs::remove_dir_all(dir);
}
