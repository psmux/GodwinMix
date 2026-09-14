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
pub mod observe;
pub mod output;
pub mod probe;
pub mod snapshot;
pub mod state;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::Config;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};

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

    /// Print per stage and per plugin start time once everything is up, and
    /// name anything that took longer than 250 ms.
    #[arg(long)]
    startup_report: bool,

    /// How log lines are written. `auto` is human form on a terminal and JSON
    /// anywhere else, which is what a service unit and a container want.
    #[arg(long, value_enum, default_value_t = observe::logs::Format::Auto)]
    log_format: observe::logs::Format,

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
    /// doctor, logs, trace, dot and support-bundle. See `src/observe/`.
    #[command(flatten)]
    Observe(observe::cli::ObserveCmd),
}

/// Parse the command line and do what it says. Both binaries call this.
pub async fn run() -> Result<()> {
    let args = Args::parse();

    observe::introspect::begin();
    // Every log line goes to stderr, which keeps stdout clean for the things
    // that are meant to be piped: MCP's protocol, and `gmx dot | dot -Tsvg`.
    // Levels start from `RUST_LOG` and move at runtime from there: see
    // `observe::logs`.
    observe::logs::init(observe::logs::Options {
        format: args.log_format,
        node: config::env_var("NODE"),
        env_filter: std::env::var("RUST_LOG").ok(),
    });

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
        Some(Command::Observe(cmd)) => {
            // These commands print a report to stdout. The mixer's own log on
            // stderr is noise around it unless the operator asked for it.
            if std::env::var_os("RUST_LOG").is_none() {
                observe::logs::set_default_level(observe::logs::LevelCode::WARN);
            }
            return observe::cli::run(cmd).await;
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

    {
        let _stage = observe::introspect::stage("gstreamer init");
        gstreamer::init().context("initialising GStreamer")?;
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
    let load = observe::introspect::stage("config");
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
    drop(load);

    let build = observe::introspect::stage("mixer build");
    let (mut mix, handle, cmd_rx, mut bus_rx) = mixer::Mixer::build(cfg)?;
    mix.persist_runtime_to(Config::runtime_store_path(&config_path));
    drop(build);

    // Logs to files, the session log, and the task that records every event.
    // After `Mixer::build`, because the recorder needs a broadcast to join, and
    // before `mix.start`, because that is where the configured sources are
    // built and their lines are exactly the ones somebody debugging a box that
    // will not come up wants in the file.
    let observe_options = observe::Options {
        config_path: config_path.clone(),
        startup_report: args.startup_report,
    };
    match observe::start(&handle, &observe_options) {
        Ok(dir) => info!(runtime_dir = %dir.display(), "logs and the session log are here"),
        Err(e) => warn!(?e, "no runtime directory, so logs stay on stderr only"),
    }

    {
        let _stage = observe::introspect::stage("mixer start");
        mix.start().context("starting mixer")?;
    }

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

    if args.startup_report {
        // stdout, because it is a report somebody asked for, not a log line.
        print!("{}", observe::introspect::format_startup_report(&observe::startup_report()));
    }

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
