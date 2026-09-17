//! The acceptance tests that need a real mixer.
//!
//! Real GStreamer elements, as every other test in this repository uses: two
//! colour bar sources on a small canvas, the whole method table in front of
//! them, and the dispatcher doing everything it does on `/rpc`. Nothing is
//! mocked, so a rule that passes here passes on air.
//!
//! What is checked:
//!
//! * 07 Phase 1: a second `program.take` inside `min_hold_ms` answers -32003
//!   with `retry_after_ms`, and `program.revert` restores the previous source.
//! * 09 section 5 item 7: every error the registry can raise carries a `data`
//!   object, says whether a retry can work, and names either a value that
//!   would have worked or a concrete next step.

use godwinmix::control::{call, methods, AppState};
use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::{self, Mixer};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::{ConfirmPolicy, Token, TokenSafety};
use godwinmix_core::snapshot::Tracker;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// Wait until `want` takes have reached `program.history`.
///
/// Bounded, and it says what it was waiting for: a take that never reaches the
/// history is a bug in the subscriber, not slowness, and the message has to
/// tell the two apart.
async fn wait_for_history(core: &Core, token: &Token, want: usize) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let history = core
            .call(token, "program.history", json!({ "limit": 10 }))
            .await
            .expect("program.history answers");
        let takes = history.as_array().map_or(0, |t| t.len());
        if takes >= want {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{want} takes were made and {takes} reached the history in 10 s. \
             program.revert reads the history, so it would refuse to go back to a \
             shot that was on air."
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A whole core: pipelines running, two sources live, the method table in
/// front of them.
struct Core {
    app: AppState,
    snapshots: Arc<Tracker>,
    registry: Registry<call::Call>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Core {
    async fn start(safety: godwinmix_core::safety::SafetyConfig) -> Core {
        Core::start_with(safety, false).await
    }

    async fn start_with(
        safety: godwinmix_core::safety::SafetyConfig,
        rehearsal: bool,
    ) -> Core {
        let _ = gstreamer::init();
        let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
        cfg.canvas.width = 320;
        cfg.canvas.height = 180;
        cfg.canvas.fps = 30;
        cfg.multiview.enabled = false;
        cfg.safety = safety;

        let (mut mix, handle, cmd_rx, mut bus_rx) =
            Mixer::build(cfg.clone()).expect("building the mixer");
        mix.start().expect("starting the mixer");
        {
            let handle = handle.clone();
            tokio::spawn(async move {
                while let Some(ev) = bus_rx.recv().await {
                    if handle.send(mixer::Command::Bus(ev)).is_err() {
                        return;
                    }
                }
            });
        }
        let multiview = mix.multiview_handle();
        let preview = mix.preview_handle();
        let encoder = mix.encoder_handle();
        let thread = mixer::spawn(mix, cmd_rx, handle.clone());
        let snapshots =
            Tracker::new(cfg.snapshot.clone(), multiview.clone(), handle.clone());

        let library = Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone()));
        let converter = Arc::new(godwinmix_core::convert::Converter::new(
            handle.clone(),
            library.cfg().convert_threads,
            library.cfg().probe_timeout_secs,
        ));
        let app = AppState::new(
            &cfg,
            godwinmix::control::Engine {
                mixer: handle.clone(),
                multiview,
                preview,
                encoder,
                library,
                converter,
                quit: Arc::new(tokio::sync::Notify::new()),
                // In memory: a live test writes no scene collection to disk.
                scenes: godwinmix_core::scene::server::SceneServer::in_memory(
                    godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
                ),
                // No plugins are installed in a live test, so the supervisor
                // has nothing to run; it is here because the control plane
                // asks it what transitions exist.
                plugins: godwinmix_core::plugin::supervisor::Supervisor::new(
                    godwinmix_core::caps::CanvasCaps::new(&cfg.canvas),
                    Default::default(),
                ),
            },
            rehearsal,
        );
        let core = Core {
            snapshots,
            registry: methods::registry(),
            app,
            thread: Some(thread),
        };
        godwinmix::control::spawn_background(core.app.clone());
        for id in ["cam1", "cam2"] {
            core.add_source(id).await;
        }
        core.wait_live("cam1").await;
        core.wait_live("cam2").await;
        core
    }

    async fn add_source(&self, id: &str) {
        let source: SourceConfig = toml::from_str(&format!(
            "id = \"{id}\"\ntype = \"test/source\"\nuri = \"test://smpte\"\n"
        ))
        .expect("a valid source document");
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.app
            .mixer
            .send(mixer::Command::AddSource(Box::new(source), Some(tx)))
            .expect("the mixer takes the source");
        rx.await.expect("the mixer answered").expect("the source was added");
    }

    /// Colour bars come up in well under a second; this waits rather than
    /// sleeping a fixed time so a slow machine does not make it flaky.
    async fn wait_live(&self, id: &str) {
        for _ in 0..200 {
            let status = self.app.mixer.status().await.expect("status");
            if status
                .sources
                .iter()
                .any(|s| s.id == id && s.state == godwinmix_protocol::types::SourceState::Live)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("{id} never went live");
    }

    async fn call(&self, token: &Token, method: &str, params: Value) -> Result<Value, RpcError> {
        call::dispatch(
            &self.registry,
            &self.app,
            &self.snapshots,
            token,
            "test-trace",
            method,
            params,
        )
        .await
    }

    async fn program(&self) -> Option<String> {
        self.app.mixer.status().await.expect("status").program
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.app.mixer.send(mixer::Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn desk() -> Token {
    Token { id: "desk".into(), ..Token::open() }
}

/// The acceptance line from 07 Phase 1, on a running mixer: a second take
/// inside the hold is refused with the time left, and reverting puts the
/// previous source back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_minimum_hold_refuses_a_second_take_and_revert_puts_the_shot_back() {
    let core = Core::start(godwinmix_core::safety::SafetyConfig {
        min_hold_ms: 8_000,
        flash_guard: false,
        ..Default::default()
    })
    .await;
    let token = desk();

    let first = core.call(&token, "program.take", json!({ "source": "cam1" })).await.unwrap();
    assert_eq!(first["program"], "cam1");
    assert_eq!(core.program().await.as_deref(), Some("cam1"));

    let refused = core
        .call(&token, "program.take", json!({ "source": "cam2" }))
        .await
        .expect_err("the hold should refuse this");
    assert_eq!(refused.code, -32003, "{refused:?}");
    let left = refused.data["retry_after_ms"].as_u64().expect("retry_after_ms");
    assert!(left > 7_000 && left <= 8_000, "{left} ms left");
    assert_eq!(refused.data["rule"], "min_hold");
    assert_eq!(refused.data["retryable"], true);
    assert!(refused.message.contains(&left.to_string()), "{}", refused.message);
    // And nothing moved.
    assert_eq!(core.program().await.as_deref(), Some("cam1"));

    // Revert is not held by the minimum hold, and it lands on the shot before.
    core.app.safety().record("desk");
    let taken = core.call(&token, "program.take", json!({ "source": "cam2" })).await;
    // The hold is still on, so take cam2 by going round it the way a vision
    // mixer with a looser token would.
    if taken.is_err() {
        let loose = Token {
            id: "vision-desk".into(),
            safety: Some(TokenSafety { min_hold_ms: Some(0), ..TokenSafety::default() }),
            ..Token::open()
        };
        core.call(&loose, "program.take", json!({ "source": "cam2" })).await.unwrap();
    }
    assert_eq!(core.program().await.as_deref(), Some("cam2"));

    // The take history is written by the subscriber on the event bus, not by
    // the call that made the take, so a shot is on air a moment before it is
    // in the log. `program.revert` reads the log. Wait for it to catch up
    // rather than racing it: on the first Windows CI run both takes were on
    // air and neither had been recorded, and revert answered "0 takes have
    // been recorded on this core".
    wait_for_history(&core, &token, 2).await;

    let reverted = core.call(&token, "program.revert", json!({})).await.unwrap();
    assert_eq!(reverted["program"], "cam1", "revert goes back to the shot before");
    assert_eq!(core.program().await.as_deref(), Some("cam1"));

    // And the history says who made each of them.
    let history = core.call(&token, "program.history", json!({ "limit": 5 })).await.unwrap();
    let takes = history.as_array().expect("a list of takes");
    assert!(takes.len() >= 2, "{takes:?}");
    assert!(takes.iter().all(|t| t["by"].is_string()), "{takes:?}");
    assert!(takes.iter().all(|t| t["at_running_time_ms"].is_number()), "{takes:?}");
}

/// A human token may loosen the limits; an agent's token may not, and the
/// rule is enforced where the take is, not where the config is read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_token_cannot_loosen_the_hold_and_a_human_token_can() {
    let core = Core::start(godwinmix_core::safety::SafetyConfig {
        min_hold_ms: 5_000,
        flash_guard: false,
        ..Default::default()
    })
    .await;
    let loose = TokenSafety { min_hold_ms: Some(0), ..TokenSafety::default() };
    let human = Token { id: "desk".into(), safety: Some(loose), ..Token::open() };
    let agent = Token { id: "studio-agent".into(), agent: true, safety: Some(loose), ..Token::open() };

    core.call(&human, "program.take", json!({ "source": "cam1" })).await.unwrap();
    // The human token asked for no hold and gets none.
    core.call(&human, "program.take", json!({ "source": "cam2" })).await.unwrap();

    // The agent asked for the same thing and is still held.
    let refused = core
        .call(&agent, "program.take", json!({ "source": "cam1" }))
        .await
        .expect_err("an agent cannot shorten its own hold");
    assert_eq!(refused.code, -32003);
    assert_eq!(refused.data["rule"], "min_hold");
}

/// 09 section 5 item 7: every refusal an agent can provoke carries a `data`
/// object, says whether a retry can work, and names either a value that would
/// have worked or a concrete next step.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_refusal_carries_data_retryable_and_a_way_forward() {
    let core = Core::start(godwinmix_core::safety::SafetyConfig {
        min_hold_ms: 60_000,
        flash_guard: false,
        ..Default::default()
    })
    .await;
    let token = desk();
    let reader = Token {
        id: "watcher".into(),
        scopes: vec![godwinmix_protocol::scope::Scope::Read],
        ..Token::open()
    };
    let careful = Token { id: "careful".into(), confirm: ConfirmPolicy::Required, ..Token::open() };
    core.call(&token, "program.take", json!({ "source": "cam1" })).await.unwrap();

    let cases: Vec<(&str, &str, Value, &Token, &str)> = vec![
        ("a source that is not there", "program.take", json!({ "source": "cam9" }), &token, "cam1"),
        ("an unknown id", "source.get", json!({ "id": "nope" }), &token, "cam1"),
        // A duplicate id is not an error here: the core takes the next free
        // id and says so in the answer, which is better than refusing a call
        // whose intent was clear. An unknown kind is the refusal an agent
        // that guessed at a type actually meets.
        ("an unknown source kind", "source.add", json!({ "id": "x", "type": "no/such", "uri": "test://smpte" }), &token, "type"),
        ("a scope miss", "program.take", json!({ "source": "cam2" }), &reader, "operate"),
        ("a safety refusal", "program.take", json!({ "source": "cam2" }), &token, "take again"),
        ("confirmation required", "source.remove", json!({ "id": "cam2" }), &careful, "confirm"),
        ("an unknown method", "source.destroy", json!({}), &token, "source.remove"),
        ("params that do not parse", "source.add", json!({}), &token, "core.api"),
        ("an unknown task", "task.get", json!({ "task_id": "nope" }), &token, "task"),
        ("an unknown output", "output.remove", json!({ "id": "nope" }), &token, "output"),
        ("an unknown filter", "filter.remove", json!({ "id": "nope" }), &token, "filter"),
        ("dry_run on a read", "source.list", json!({ "dry_run": true }), &token, "without dry_run"),
    ];

    for (what, method, params, who, expected) in cases {
        let e = match core.call(who, method, params.clone()).await {
            Err(e) => e,
            Ok(body) => panic!("{what} ({method}) was accepted: {body}"),
        };
        assert!(e.data.is_object(), "{what}: data is not an object: {:?}", e.data);
        assert!(
            e.data.get("retryable").is_some_and(Value::is_boolean),
            "{what}: data has no retryable: {:?}",
            e.data
        );
        assert!(
            e.message.len() > 20,
            "{what}: the message is too short to say anything: {}",
            e.message
        );
        assert!(
            e.message.to_lowercase().contains(&expected.to_lowercase()),
            "{what}: the message does not name a value that would work or a next step \
             (looking for {expected:?}):\n{}",
            e.message
        );
        // Every message ends in an instruction rather than a description, so
        // it has at least two sentences: what is wrong, and what to do.
        assert!(
            e.message.matches(['.', '?']).count() >= 2 || e.message.contains(':'),
            "{what}: one sentence is a description, not a repair instruction:\n{}",
            e.message
        );
    }

    // And the rehearsal refusal, which needs a core started for it. The
    // credential half is covered in `scope.rs`; this is the method half.
    let refusal = core
        .call(&token, "output.add", json!({ "id": "yt", "url": "rtmp://127.0.0.1/live/x" }))
        .await;
    // A live core accepts output.add, so this must not be a safety refusal.
    if let Err(e) = refusal {
        assert_ne!(e.data.get("rehearsal"), Some(&json!(true)), "this is a live core");
    }
}

/// 09 section 5 item 14, the method half: a core started with `--rehearsal`
/// will not add an output, so an agent rehearsing cannot put anything on a
/// real destination by accident. The credential half is in `scope.rs`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rehearsal_core_refuses_to_add_an_output_and_does_everything_else() {
    let core = Core::start_with(
        godwinmix_core::safety::SafetyConfig { min_hold_ms: 0, flash_guard: false, ..Default::default() },
        true,
    )
    .await;
    let token = desk();

    let refusal = core
        .call(&token, "output.add", json!({ "id": "yt", "url": "rtmp://127.0.0.1/live/x" }))
        .await
        .expect_err("a rehearsal core must not add an output");
    assert_eq!(refusal.code, -32003, "{refusal:?}");
    assert_eq!(refusal.data["rehearsal"], true);
    assert!(refusal.message.contains("--rehearsal"), "{}", refusal.message);
    assert!(
        refusal.message.contains("Everything else works"),
        "the refusal has to say what still works: {}",
        refusal.message
    );

    // And everything else does work, which is the point of rehearsing.
    let taken = core.call(&token, "program.take", json!({ "source": "cam1" })).await.unwrap();
    assert_eq!(taken["program"], "cam1");
    assert!(core.call(&token, "core.info", json!({})).await.unwrap()["rehearsal"] == true);
}

/// The task surface, end to end through the dispatcher: an unknown id names
/// the ids that exist, and a task read back carries the shape 03 section 6
/// describes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_task_is_readable_and_cancellable_through_the_method_table() {
    let core = Core::start(godwinmix_core::safety::SafetyConfig::default()).await;
    let token = desk();

    let missing = core
        .call(&token, "task.get", json!({ "task_id": "nope" }))
        .await
        .expect_err("an unknown task is refused");
    assert_eq!(missing.code, -32004);
    assert_eq!(missing.data["kind"], "task");

    let id = core.app.tasks.spawn("plugin.add", |ctx| async move {
        for _ in 0..200 {
            if ctx.cancelled() {
                return Ok(json!({ "stopped": true }));
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Ok(json!({ "installed": true }))
    });

    let view = core.call(&token, "task.get", json!({ "task_id": &id })).await.unwrap();
    assert_eq!(view["state"], "running");
    assert_eq!(view["kind"], "plugin.add");
    assert_eq!(view["poll_interval_ms"], 1_000);

    core.call(&token, "task.cancel", json!({ "task_id": &id })).await.unwrap();
    for _ in 0..200 {
        let view = core.call(&token, "task.get", json!({ "task_id": &id })).await.unwrap();
        if view["state"] != "running" {
            assert_eq!(view["state"], "cancelled");
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the task never stopped");
}
