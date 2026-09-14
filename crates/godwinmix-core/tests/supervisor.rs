//! The supervisor, against a real plugin process.
//!
//! Everything here runs a shell script that speaks the published protocol,
//! because a mock would prove only that the mock and the core agree. The
//! fixture is `tests/fixtures/fake-service`: one `service` provide, one
//! `device` provide, one `[[tools]]` entry, and an `event` notification that
//! has to become a live source.
//!
//! The acceptance these serve, from 07 Phase 2: *an OBS instance or a phone
//! publishing to an ingest plugin appears as a live source within five
//! seconds with no configuration.* The fixture stands in for the publisher;
//! what is measured is the core's half.

#![cfg(unix)]

use godwinmix_core::caps::CanvasCaps;
use godwinmix_core::plugin::loader;
use godwinmix_core::plugin::supervisor::Supervisor;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Where the fixture lives in the checkout.
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-service")
}

fn temp(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("gmx-supervisor-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a temporary directory");
    path
}

/// One lock for the file: the plugin registry is one per process.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Install the fixture into a plugins directory of this test's own.
fn install(tag: &str) -> PathBuf {
    let dir = temp(tag);
    loader::set_dir(dir.clone());
    loader::set_runtime_dir(dir.join("run"));
    loader::install_from_path(&fixture()).expect("installing the fixture");
    dir
}

fn canvas() -> CanvasCaps {
    CanvasCaps::new(&godwinmix_core::config::Canvas {
        width: 320,
        height: 180,
        fps: 30,
        sample_rate: 48_000,
        channels: 2,
    })
}

/// `[plugins.fakeservice]`, as an operator's config would carry it.
fn settings(announce: Option<&str>) -> BTreeMap<String, godwinmix_core::config::Params> {
    let mut params = godwinmix_core::config::Params::new();
    if let Some(id) = announce {
        params.insert("announce".into(), toml::Value::String(id.to_string()));
    }
    [("fakeservice".to_string(), params)].into_iter().collect()
}

/// The same, for a condition that has to be awaited.
async fn until_async<F, Fut>(what: &str, within: Duration, mut ready: F) -> Duration
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = Instant::now();
    while start.elapsed() < within {
        if ready().await {
            return start.elapsed();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{what} did not happen within {} ms", within.as_millis());
}

/// Descriptors held by this process, for the leak count.
fn descriptors() -> usize {
    let dir = if cfg!(target_os = "macos") { "/dev/fd" } else { "/proc/self/fd" };
    std::fs::read_dir(dir).map(|d| d.count()).unwrap_or(0)
}

/// A service is a singleton, named after its provide, started with the core.
#[test]
fn a_service_and_a_device_are_started_as_one_instance_each() {
    let _lock = exclusive();
    let dir = install("start");
    let supervisor = Supervisor::new(canvas(), settings(None));
    let failures = supervisor.start_all();
    assert!(failures.is_empty(), "a singleton would not start: {failures:?}");

    let instances = supervisor.instances();
    let names: Vec<&str> = instances.iter().map(|(name, _, _, _)| name.as_str()).collect();
    assert!(
        names.contains(&"fakeservice-service"),
        "the service must be one instance named after its provide: {names:?}"
    );
    assert!(names.contains(&"fakeservice-discovery"), "and so must the device: {names:?}");
    assert_eq!(instances.len(), 2, "one instance per provide and no more: {names:?}");

    // Starting again is not a second process. A `plugin.add` on a core that
    // already has this plugin must leave what is running alone.
    supervisor.start_all();
    assert_eq!(supervisor.instances().len(), 2);

    // And they are in the stats table, so `plugin.list` and the budget sampler
    // can see them. A singleton that nothing can see is one nothing can hold
    // to a limit.
    let stats = loader::stats();
    assert!(
        stats.iter().any(|s| s.instance == "fakeservice-service" && s.pid.is_some()),
        "a running singleton must carry a pid in plugin.stats"
    );

    supervisor.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

/// The one a plugin author cares about: a tool call reaches the process and
/// the answer comes back in MCP's shape.
#[test]
fn a_tool_call_reaches_the_plugin_and_comes_back() {
    let _lock = exclusive();
    let dir = install("tool");
    let supervisor = Supervisor::new(canvas(), settings(None));
    supervisor.start_all();

    let answer = supervisor
        .tool_call("fakeservice/echo", serde_json::json!({"say": "hello"}))
        .expect("the tool call");
    assert_eq!(answer["content"][0]["text"], "echo");
    assert_eq!(answer["isError"], false);

    // The bare name works too, because only one plugin has it.
    assert!(supervisor.tool_call("echo", serde_json::json!({})).is_ok());

    // And a name nobody has says what is running rather than failing blankly.
    let e = supervisor
        .tool_call("nothing/at-all", serde_json::json!({}))
        .expect_err("no such tool");
    let message = format!("{e:#}");
    assert!(message.contains("no tool called"), "{message}");
    assert!(message.contains("fakeservice/echo"), "the message must list what is there: {message}");

    supervisor.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

/// `device.discover` reaches every device and merges what they find.
#[test]
fn discover_asks_every_device_and_merges_the_answers() {
    let _lock = exclusive();
    let dir = install("discover");
    let supervisor = Supervisor::new(canvas(), settings(None));
    supervisor.start_all();

    let found = supervisor.discover(Duration::from_secs(2));
    assert_eq!(found.len(), 1, "the one device offers one candidate: {found:?}");
    assert_eq!(found[0].name, "Fake Camera 1");
    assert_eq!(found[0].kind, "test/source");
    assert_eq!(found[0].params["uri"], "test://smpte".replace("smpte", "ball"));

    supervisor.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

/// The roadmap's acceptance line, with the fixture standing in for a phone:
/// something that turns up becomes a live source, and quickly.
#[tokio::test(flavor = "multi_thread")]
async fn a_device_publisher_becomes_a_live_source_within_five_seconds() {
    let _lock = exclusive();
    let _ = gstreamer::init();
    let dir = install("adopt");

    // A real mixer on its own thread, exactly as the binary runs one.
    let mut cfg: godwinmix_core::config::Config = toml::from_str(
        "[canvas]\nwidth = 320\nheight = 180\nfps = 30\nsample_rate = 48000\nchannels = 2\n\n[multiview]\nenabled = false\n",
    )
    .expect("a three line config");
    cfg.program.encoder = "on-demand".into();
    let (mut mixer, handle, commands, _bus) =
        godwinmix_core::mixer::Mixer::build(cfg).expect("the mixer builds");
    mixer.start().expect("the programme starts");
    let thread = godwinmix_core::mixer::spawn(mixer, commands, handle.clone());

    let supervisor = Supervisor::new(canvas(), settings(Some("guest")));
    supervisor.attach(handle.clone());
    supervisor.start_all();
    supervisor.spawn_pump();

    // The plugin raised `source.appeared` as soon as it was up. What is being
    // measured is everything after that: the pump draining the notice, the
    // supervisor turning a candidate into a source config, the mixer building
    // the source and the source going live.
    let live = |handle: godwinmix_core::mixer::MixerHandle| async move {
        handle
            .status()
            .await
            .map(|s| {
                s.sources.iter().any(|source| {
                    source.id == "guest"
                        && source.state == godwinmix_core::state::SourceState::Live
                })
            })
            .unwrap_or(false)
    };
    let took = until_async("a publisher becoming a live source", Duration::from_secs(5), || {
        live(handle.clone())
    })
    .await;
    println!(
        "a device's publisher became a live source in {} ms; the bar is 5000 ms",
        took.as_millis()
    );

    // And when it goes away again, so does the source. Only what a device
    // added: an operator's own camera is not a device's to remove.
    // Named at the device, because a plugin with two instances answers the
    // same tool from both and it is the device's event stream the core routes.
    supervisor
        .tool_call("fakeservice/discovery/echo", serde_json::json!({"take_it_away": true}))
        .expect("the tool call that makes it leave");
    until_async("the source going away again", Duration::from_secs(5), || {
        let handle = handle.clone();
        async move { !live(handle).await }
    })
    .await;

    supervisor.shutdown();
    let _ = handle.send(godwinmix_core::mixer::Command::Shutdown);
    let _ = thread.join();
    let _ = std::fs::remove_dir_all(dir);
}

/// `plugin.add` then `plugin.remove` leaves nothing: no process, no
/// descriptor, no registry row, no directory.
///
/// The service half of the leak test in `sidecar.rs`, and the half that was
/// missing: a singleton's instance id is `<plugin>-<provide>`, which does not
/// start with the plugin's name, so the rows keyed by instance survived a
/// removal that pruned by prefix.
#[test]
fn a_service_leaves_nothing_behind_when_the_plugin_is_removed() {
    let _lock = exclusive();
    let before = descriptors();
    let dir = install("leak");
    let supervisor = Supervisor::new(canvas(), settings(None));
    supervisor.start_all();
    let pids: Vec<u32> = loader::stats().into_iter().filter_map(|s| s.pid).collect();
    assert_eq!(pids.len(), 2, "two singletons, two processes");

    supervisor.stop_plugin("fakeservice", "the plugin is being removed");
    assert!(supervisor.instances().is_empty(), "stopping must forget the instances");
    loader::uninstall("fakeservice").expect("uninstalling");

    // Nothing in the registry.
    assert!(
        loader::stats().iter().all(|s| s.plugin != "fakeservice"),
        "a removed plugin left rows in plugin.stats"
    );
    assert!(loader::get("fakeservice").is_none());
    assert!(
        !dir.join("fakeservice").exists(),
        "plugin.add then plugin.remove must leave no directory"
    );

    // No processes. `kill -0` answers whether the pid is still ours to signal.
    for pid in pids {
        let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
        assert!(!alive, "the plugin's process {pid} is still running after plugin.remove");
    }

    // And no descriptors. The same slack the source leak test allows, because
    // a test binary opens and closes files of its own around this.
    let after = descriptors();
    assert!(
        after <= before + 8,
        "a service leaked descriptors: {before} before, {after} after"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A reload swaps the instances one at a time, and a new version that will not
/// start leaves the previous one running.
#[test]
fn a_reload_swaps_the_instances_and_rolls_back_when_the_new_one_will_not_start() {
    let _lock = exclusive();
    let dir = install("reload");
    let supervisor = Supervisor::new(canvas(), settings(None));
    supervisor.start_all();
    let first: Vec<u32> = loader::stats().into_iter().filter_map(|s| s.pid).collect();
    assert_eq!(first.len(), 2);

    let swapped = supervisor.reload("fakeservice");
    assert_eq!(swapped.swapped.len(), 2, "both instances should have been swapped");
    assert!(swapped.failed.is_empty(), "{:?}", swapped.failed);
    let second: Vec<u32> = loader::stats().into_iter().filter_map(|s| s.pid).collect();
    assert_eq!(second.len(), 2);
    assert!(
        first.iter().all(|pid| !second.contains(pid)),
        "a reload that swapped nothing: {first:?} then {second:?}"
    );
    for pid in &first {
        let alive = unsafe { libc::kill(*pid as i32, 0) } == 0;
        assert!(!alive, "the previous instance {pid} was left running by the swap");
    }

    // Now break the plugin and reload again. The new instance cannot hand
    // shake, so the previous one has to come back rather than leaving a hole.
    let entry = dir.join("fakeservice/0.1.0/run.sh");
    std::fs::write(&entry, "#!/bin/sh\nexit 1\n").expect("breaking the plugin");
    let broken = supervisor.reload("fakeservice");
    assert!(!broken.failed.is_empty(), "a plugin that cannot start must be reported");
    assert_eq!(
        supervisor.instances().len(),
        2,
        "a failed reload must leave the instances in the table, not drop them"
    );

    supervisor.shutdown();
    let _ = loader::uninstall("fakeservice");
    let _ = std::fs::remove_dir_all(dir);
}
