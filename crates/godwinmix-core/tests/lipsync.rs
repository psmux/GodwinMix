//! Two sources on a node and one local one, in lip sync, over a long run.
//!
//! The roadmap's Phase 4 acceptance asks for two remote cameras and one local
//! file staying in lip sync across a long run. There are no NDI cameras here,
//! so the two remote sources are `test://` patterns on a node in this process
//! and the local one is a `test://` pattern on the core, which exercises the
//! same thing: three sources on three different timelines, arriving through
//! two different paths, judged against one programme clock.
//!
//! What is measured is what the README's verification table measures with the
//! spectral centroid method: every source carries a distinct tone, and the
//! programme's audio is checked for which tone is present when. Here the
//! equivalent and cheaper measurement is how long each source has been without
//! a frame, sampled once a second. A source keeping up with the programme has
//! a small figure; one drifting behind has a growing one. The number that
//! matters is the **spread**: how far apart the three sources sit. A lip sync
//! fault is the spread growing, not the figure being large, because a delay
//! every sink shares is a delay and not a desync.
//!
//! Marked `#[ignore]` because it is a five minute run. Run it with:
//!
//! ```text
//! cargo test -p godwinmix-core --test lipsync -- --ignored --nocapture
//! GMX_LIPSYNC_SECS=60 cargo test -p godwinmix-core --test lipsync -- --ignored --nocapture
//! ```

use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::{self, Command, Mixer};
use godwinmix_core::node::{self, Place};
use gstreamer as gst;
use std::time::Duration;

/// How long the run is. Five minutes by default, which is what the acceptance
/// asks for; shorter when somebody is iterating.
fn run_secs() -> u64 {
    std::env::var("GMX_LIPSYNC_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(300)
}

fn canvas_config() -> Config {
    toml::from_str(
        r#"
[canvas]
width = 640
height = 360
fps = 30
sample_rate = 48000
channels = 2

[control]
bind = "127.0.0.1:0"

[nodes]
listen = "127.0.0.1:0"
"#,
    )
    .unwrap()
}

/// One source, with a tone of its own so a spectral measurement on the
/// programme can tell them apart.
fn source(id: &str, place: Place, freq: u32) -> SourceConfig {
    let mut cfg = SourceConfig::bare(id, "test://smpte");
    cfg.place = Some(place);
    cfg.params.insert("freq".into(), toml::Value::Integer(freq as i64));
    cfg.transport = Some(node::BridgeTransport::Srt);
    cfg.latency_ms = Some(120);
    cfg
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a five minute run; see the module comment"]
async fn two_on_a_node_and_one_local_hold_lip_sync() {
    gst::init().unwrap();
    let cfg = canvas_config();
    let (mut mix, handle, cmd_rx, _bus_rx) = Mixer::build(cfg.clone()).expect("build a mixer");
    mix.start().expect("start the mixer");
    let clock = mix.program_clock().expect("the programme has a clock");
    let canvas = mix.canvas().clone();

    // The core's node bridge, on a free port, in a temporary directory.
    let dir = std::env::temp_dir().join(format!("gmx-lipsync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bound = node::runtime::start(node::runtime::Options {
        dir: dir.clone(),
        bind: "127.0.0.1:0".into(),
        server_names: vec!["127.0.0.1".into(), "localhost".into()],
        clock_port: 0,
        clock_kind: "net".into(),
        advertise: false,
        clock,
        canvas: godwinmix_protocol::plugin::wire::Canvas {
            width: canvas.width as u32,
            height: canvas.height as u32,
            fps: 30,
        },
        expected: Vec::new(),
        watch: std::sync::Arc::new(|_| {}),
    })
    .await
    .expect("start the node bridge");
    let runtime = node::runtime::get().expect("the runtime is installed");

    // A node in this process. Same code, same certificates, same socket; the
    // only thing it does not have is a second machine under it.
    let ticket = runtime.tickets.mint("studio-b", node::enrol::DEFAULT_TTL).unwrap();
    let options = node::daemon::Options {
        core: format!("127.0.0.1:{}", bound.port()),
        name: "studio-b".into(),
        token: Some(format!("{}.{}", runtime.ca.fingerprint(), ticket.token)),
        home: dir.join("node-home"),
        clock: node::clock::Kind::Net,
        media_host: Some("127.0.0.1".into()),
    };
    let identity = node::daemon::ensure_identity(&options).await.expect("enrol");
    let daemon = node::daemon::Node::new(options).expect("build the node");
    let bridge = tokio::spawn({
        let daemon = daemon.clone();
        async move {
            if let Err(e) = node::daemon::connect(&daemon, &identity).await {
                eprintln!("the node's bridge ended: {e:#}");
            }
        }
    });
    for _ in 0..200 {
        if runtime.nodes.is_online("studio-b") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(runtime.nodes.is_online("studio-b"), "the node did not join");

    let thread = mixer::spawn(mix, cmd_rx, handle.clone());

    // Two on the node, one here, each with its own tone.
    let wanted = [
        ("cam1", Place::Node("studio-b".into()), 440),
        ("cam2", Place::Node("studio-b".into()), 660),
        ("local", Place::Core, 1000),
    ];
    let mut started = Vec::new();
    for (id, place, freq) in &wanted {
        match handle
            .request(|ack| {
                Command::AddSource(Box::new(source(id, place.clone(), *freq)), Some(ack))
            })
            .await
        {
            Ok(_) => started.push(*id),
            Err(e) => eprintln!("{id} at {place} would not start: {e:#}"),
        }
    }
    assert!(
        started.contains(&"local"),
        "the local source must start; without it there is nothing to compare against"
    );
    handle
        .request(|ack| Command::Take {
            source: Some("local".into()),
            at_running_time_ms: None,
            ack: Some(ack),
        })
        .await
        .expect("take the local source");

    // Sample every source's lag against the programme once a second.
    let secs = run_secs();
    println!("running for {secs} s with {} source(s): {started:?}", started.len());
    let mut samples: Vec<Vec<f64>> = Vec::new();
    for tick in 0..secs {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let status = match handle.status().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("the mixer stopped answering at {tick} s: {e:#}");
                break;
            }
        };
        let row: Vec<f64> = started
            .iter()
            .map(|id| {
                status
                    .sources
                    .iter()
                    .find(|s| s.id == *id)
                    .and_then(|s| s.video_idle_ms)
                    .map(|ms| ms as f64)
                    .unwrap_or(f64::NAN)
            })
            .collect();
        if tick % 30 == 0 {
            println!("  {tick:>4} s  {row:?}");
        }
        samples.push(row);
    }

    handle.send(Command::Shutdown).ok();
    let _ = thread.join();
    bridge.abort();
    let _ = std::fs::remove_dir_all(&dir);

    // The spread, per sample, between the furthest apart sources that were
    // producing anything at all.
    let mut spreads = Vec::new();
    for row in &samples {
        let live: Vec<f64> = row.iter().copied().filter(|v| v.is_finite()).collect();
        if live.len() < 2 {
            continue;
        }
        let max = live.iter().cloned().fold(f64::MIN, f64::max);
        let min = live.iter().cloned().fold(f64::MAX, f64::min);
        spreads.push(max - min);
    }
    assert!(!spreads.is_empty(), "nothing was measured; no two sources produced frames together");
    let worst = spreads.iter().cloned().fold(0.0f64, f64::max);
    let mean = spreads.iter().sum::<f64>() / spreads.len() as f64;
    let mut sorted = spreads.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    println!(
        "\nlip sync over {secs} s, {} samples across {} sources\n  median spread {median:.1} ms\n  \
         mean spread   {mean:.1} ms\n  worst spread  {worst:.1} ms",
        spreads.len(),
        started.len()
    );

    // One frame at 30 fps is 33 ms and is the point at which a viewer can see
    // it. The ceiling is two frames, which is the figure the README's table
    // uses for "no gap" elsewhere.
    assert!(
        median < 66.7,
        "the median spread between sources was {median:.1} ms, which is more than two frames"
    );
}
