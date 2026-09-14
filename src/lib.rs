//! GodwinMix: a live RTMP video mixer.
//!
//! Multiple RTMP sources come in, one of them is on program at a time, and the
//! program feed goes out to one or more RTMP destinations without ever
//! stopping. Switching source is instant and does not disturb the outgoing
//! stream, because the output encoder is started once and runs for the life of
//! the broadcast; everything that changes happens upstream of it in raw video.
//!
//! See `mixer.rs` for why that arrangement is the whole design.
//!
//! The program is a library with two thin binaries over it, `godwinmix` and
//! `gmx`, because the short name is what an operator types and neither should
//! be a copy of the other. `run` below is what both call.

pub mod caps;
pub mod config;
pub mod convert;
pub mod control;
pub mod ctl;
pub mod gstutil;
pub mod input;
pub mod mcp;
pub mod media;
pub mod mixer;
pub mod multiview;
pub mod output;
pub mod plugin;
pub mod probe;
pub mod snapshot;
pub mod state;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::Config;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

const EXAMPLE_CONFIG: &str = include_str!("../godwinmix.example.toml");

/// Where a client subcommand looks for a mixer when nothing says otherwise.
const DEFAULT_URL: &str = "http://127.0.0.1:8080";

#[derive(Parser, Debug)]
#[command(name = "godwinmix", about = "Live RTMP video mixer with hot source switching")]
struct Args {
    /// Path to the TOML configuration file.
    #[arg(short, long, default_value = "godwinmix.toml")]
    config: PathBuf,

    /// Override the control server bind address from the config.
    #[arg(short, long)]
    bind: Option<String>,

    /// Print a commented example configuration and exit.
    #[arg(long)]
    example_config: bool,

    /// Report the codec backends that would be selected on this machine, then
    /// exit. Useful for checking a new server before pointing cameras at it.
    #[arg(long)]
    probe: bool,

    /// Run the conformance harness against every built in source kind that
    /// needs no network, print a report, and exit non-zero if any check fails.
    ///
    /// A core with no outputs, no multiview and a 1280x720x30 canvas, which is
    /// what 03 section 11 calls the test core. It is what `gmx plugin test`
    /// will spawn, and it is here so the same checks a plugin author runs are
    /// the ones the core runs against itself.
    #[arg(long)]
    test_core: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Control a running mixer over its HTTP API.
    Ctl {
        /// Address of the mixer's control server [default: http://127.0.0.1:8080].
        #[arg(long, env = "GODWINMIX_URL")]
        url: Option<String>,
        /// Bearer token, when the mixer has one configured.
        #[arg(long, env = "GODWINMIX_TOKEN", hide_env_values = true)]
        token: Option<String>,
        #[command(subcommand)]
        cmd: ctl::Ctl,
    },
    /// Expose a running mixer to AI agents as Model Context Protocol tools.
    ///
    /// Speaks MCP over stdio: JSON-RPC requests one per line on stdin,
    /// replies on stdout. An MCP client such as Claude Code starts this as
    /// a child process; nothing but the protocol goes to stdout.
    Mcp {
        /// Address of the mixer's control server [default: http://127.0.0.1:8080].
        #[arg(long, env = "GODWINMIX_URL")]
        url: Option<String>,
        /// Bearer token for a mixer whose API requires one.
        #[arg(long, env = "GODWINMIX_TOKEN")]
        token: Option<String>,
    },
}

/// Run the conformance harness and print what it found.
///
/// One block per kind, `ok` or `FAIL` per check. Exits non-zero on any
/// failure, so CI and `gmx plugin test` can both read the exit code.
fn run_test_core() -> Result<()> {
    println!("godwinmix test core: 1280x720x30, no outputs, no multiview");
    let mut failures = 0usize;
    for outcome in plugin::harness::check_offline_kinds() {
        match outcome {
            Ok(report) => {
                println!("\n{}", report.type_id);
                for line in report.lines() {
                    println!("  {line}");
                }
                if !report.passed() {
                    failures += 1;
                }
            }
            Err(e) => {
                println!("\nharness could not run: {e:#}");
                failures += 1;
            }
        }
    }
    println!();
    if failures > 0 {
        anyhow::bail!("{failures} of the built in kinds failed the harness");
    }
    println!("every built in kind that runs without a network is conformant");
    Ok(())
}

/// Parse the command line and do what it says. Both binaries call this.
pub async fn run() -> Result<()> {
    let args = Args::parse();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // The MCP client reads stdout as protocol, so a log line there would
    // break the connection. Everything else keeps the usual stdout logging.
    if matches!(args.command, Some(Command::Mcp { .. })) {
        tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    // The client subcommands talk to an already-running mixer and need none
    // of the setup below.
    // clap reads the new environment variable names on its own; the old ones
    // are picked up here, with the warning, for one release. See `config::env_var`.
    match args.command {
        Some(Command::Ctl { url, token, cmd }) => {
            let url = url.or_else(|| config::env_var("URL")).unwrap_or_else(|| DEFAULT_URL.into());
            let token = token.or_else(|| config::env_var("TOKEN"));
            return ctl::run(&url, token.as_deref(), cmd).await;
        }
        Some(Command::Mcp { url, token }) => {
            let url = url.or_else(|| config::env_var("URL")).unwrap_or_else(|| DEFAULT_URL.into());
            let token = token.or_else(|| config::env_var("TOKEN"));
            return mcp::run(&url, token).await;
        }
        None => {}
    }

    if args.example_config {
        print!("{EXAMPLE_CONFIG}");
        return Ok(());
    }

    // Before anything is started: the mixer is PID 1 in its container and
    // inherits every orphan on the box, and a sidecar's grandchildren are
    // orphaned the moment their parent is killed. See `reap_orphans_if_init`.
    input::reap_orphans_if_init();

    gstreamer::init().context("initialising GStreamer")?;

    if args.test_core {
        return run_test_core();
    }

    if args.probe {
        let b = probe::Backends::probe(config::Accel::Auto, config::Accel::Auto)?;
        println!("video decoder : {}  ({:?})", b.video_decode.element, b.video_decode.accel);
        println!("video encoder : {}  ({:?})", b.video_encode.element, b.video_encode.accel);
        println!("audio decoder : {}", b.audio_decode);
        println!("audio encoder : {}", b.audio_encode);
        return Ok(());
    }

    // The LiveboxMix config name is still read when there is no GodwinMix one.
    let config_path = config::path_in_force(&args.config);
    let cfg = Config::load(&config_path).with_context(|| {
        format!(
            "could not load {}. Run with --example-config to print a starting point.",
            config_path.display()
        )
    })?;
    let bind = args.bind.unwrap_or_else(|| cfg.control.bind.clone());
    let token = cfg.token().map(Arc::from);
    match &token {
        Some(_) => info!("control API requires a bearer token"),
        None => info!("control API is open: no token configured"),
    }
    let cfg_media = cfg.media.clone();

    let (mut mix, handle, cmd_rx, mut bus_rx) = mixer::Mixer::build(cfg)?;
    mix.persist_runtime_to(Config::runtime_store_path(&config_path));
    mix.start().context("starting mixer")?;

    let frames = mix.multiview_sender().map(Arc::new);

    // Bus messages from every pipeline are funnelled into the same command
    // queue the operator's requests use, so the mixer handles a camera dying
    // and a take through one serialised path and never races with itself.
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

    let mixer_thread = mixer::spawn(mix, cmd_rx, handle.clone());

    let library = Arc::new(media::MediaLibrary::new(cfg_media));
    let converter = Arc::new(convert::Converter::new(
        handle.clone(),
        library.cfg().convert_threads,
        library.cfg().probe_timeout_secs,
    ));
    let quit = Arc::new(tokio::sync::Notify::new());
    let state = control::AppState {
        mixer: handle.clone(),
        frames,
        library,
        converter,
        quit: quit.clone(),
        token,
    };
    let server = tokio::spawn(async move {
        if let Err(e) = control::serve(&bind, state).await {
            error!(?e, "control server stopped");
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = quit.notified() => {}
    }
    info!("shutting down");
    let _ = handle.send(mixer::Command::Shutdown);
    server.abort();
    let _ = tokio::task::spawn_blocking(move || mixer_thread.join()).await;
    Ok(())
}
