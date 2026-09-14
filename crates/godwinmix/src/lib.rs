//! GodwinMix: a live RTMP video mixer.
//!
//! Multiple RTMP sources come in, one of them is on program at a time, and the
//! program feed goes out to one or more RTMP destinations without ever
//! stopping. Switching source is instant and does not disturb the outgoing
//! stream, because the output encoder is started once and runs for the life of
//! the broadcast; everything that changes happens upstream of it in raw video.
//!
//! See `godwinmix-core` for the engine underneath: this crate is everything
//! that serves it. The control plane and its method handlers, the REST and
//! WebSocket layers, the `gmx ctl` client, the MCP server, the web UI, the
//! bench command and the command line all live here, over
//! `godwinmix_core` for the mixing and `godwinmix_protocol` for the contract.
//!
//! The program is a library with two thin binaries over it, `godwinmix` and
//! `gmx`, because the short name is what an operator types and neither should
//! be a copy of the other. `run` below is what both call.

pub mod bench;
pub mod cli;
pub mod control;
pub mod ctl;
pub mod mcp;
pub mod observe;
pub mod ui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use godwinmix_core::config::{self, Config};
use godwinmix_core::observe::logs;
use godwinmix_core::{convert, input, media, mixer, observe as core_observe, plugin};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};

const EXAMPLE_CONFIG: &str = include_str!("../../../godwinmix.example.toml");

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

    /// Read an extra codec catalogue over the built in one. The same shape as
    /// the shipped `codecs.toml`; entries whose id matches replace, the rest
    /// are added. Applied after the config's `[codecs]` table.
    #[arg(long)]
    codecs: Option<PathBuf>,

    /// Run the conformance harness against every built in source kind that
    /// needs no network, print a report, and exit non-zero if any check fails.
    ///
    /// A core with no outputs, no multiview and a 1280x720x30 canvas, which is
    /// what 03 section 11 calls the test core. It is what `gmx plugin test`
    /// will spawn, and it is here so the same checks a plugin author runs are
    /// the ones the core runs against itself.
    #[arg(long)]
    test_core: bool,
    /// Print per stage and per plugin start time once everything is up, and
    /// name anything that took longer than 250 ms.
    #[arg(long)]
    startup_report: bool,

    /// How log lines are written. `auto` is human form on a terminal and JSON
    /// anywhere else, which is what a service unit and a container want.
    #[arg(long, value_enum, default_value_t = LogFormat::Auto)]
    log_format: LogFormat,

    /// Print the whole control protocol as JSON Schema and exit: every
    /// method, event and type, with `api_level`. This is `protocol.json`, and
    /// it is what `core.api` answers with. Needs no config and no GStreamer.
    #[arg(long)]
    api_info: bool,

    /// With `--api-info`, print the human readable reference instead of the
    /// JSON. This is `protocol.md`.
    #[arg(long)]
    markdown: bool,

    /// With `--api-info`, print the OpenAPI 3.1 description of the REST layer
    /// instead. This is `openapi.json`.
    #[arg(long)]
    openapi: bool,

    /// Refuse `output.add`, and accept only tokens marked `rehearsal`.
    ///
    /// An agent behaves differently when it believes a show is real, and it
    /// guesses wrong most of the time, so the guess must not matter: the
    /// credential decides which core it belongs to. A live core refuses a
    /// rehearsal token outright and this one refuses a live token.
    #[arg(long)]
    rehearsal: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

/// The `--log-format` flag, as clap spells it.
///
/// `godwinmix_core::observe::logs::Format` is the same three choices without a
/// command line parser in the engine's dependency tree; this converts into it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
enum LogFormat {
    /// Human form when stderr is a terminal, JSON when it is not.
    Auto,
    Json,
    Human,
}

impl From<LogFormat> for logs::Format {
    fn from(f: LogFormat) -> Self {
        match f {
            LogFormat::Auto => Self::Auto,
            LogFormat::Json => Self::Json,
            LogFormat::Human => Self::Human,
        }
    }
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
    /// Read a scene collection from another mixer.
    Import {
        #[command(subcommand)]
        cmd: cli::scene::Import,
    },
    /// Work with scene documents: validate, resolve a layout, convert between
    /// the nested document and the flat record store.
    Scene {
        #[command(subcommand)]
        cmd: cli::scene::Scene,
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
        /// How many tools to put in front of the agent.
        ///
        /// `standard` is twelve hot tools, about 4,000 tokens. `minimal` is
        /// five, for a model with a small context; everything else is still
        /// callable by name and findable with `search_tools`.
        #[arg(long, env = "GODWINMIX_MCP_PROFILE", default_value = "standard")]
        profile: McpProfile,
    },
    /// Inspect and test the codec catalogue.
    ///
    /// The catalogue is `codecs.toml`: which codec the programme is encoded
    /// in, which element does it, and what that element wants set. These
    /// subcommands need no running mixer.
    Codec {
        #[command(subcommand)]
        cmd: cli::codec::Codec,
    },

    /// Write, test, install and inspect plugins. See `src/cli/plugin.rs`.
    ///
    /// `new` and `test` need no running mixer; everything else is a thin
    /// client of the `plugin.*` methods, which is the same contract a third
    /// party surface calls.
    Plugin {
        /// Address of the mixer's control server [default: http://127.0.0.1:8080].
        #[arg(long, env = "GODWINMIX_URL")]
        url: Option<String>,
        /// Bearer token, when the mixer has one configured.
        #[arg(long, env = "GODWINMIX_TOKEN", hide_env_values = true)]
        token: Option<String>,
        #[command(subcommand)]
        cmd: cli::plugin::Plugin,
    },

    /// Break something on purpose and watch what the programme does.
    ///
    /// Refused on a core that is not in rehearsal unless `--i-am-sure`. See
    /// `src/cli/chaos.rs`.
    Chaos {
        #[arg(long, env = "GODWINMIX_URL")]
        url: Option<String>,
        #[arg(long, env = "GODWINMIX_TOKEN", hide_env_values = true)]
        token: Option<String>,
        #[command(subcommand)]
        cmd: cli::chaos::Chaos,
    },

    /// doctor, logs, trace, dot and support-bundle. See `src/cli/observe.rs`.
    #[command(flatten)]
    Observe(cli::observe::ObserveCmd),

    /// Measure this machine's footprint and print the budget table.
    ///
    /// Every performance number GodwinMix publishes comes from here, with the
    /// machine, the commit and the command that produced each row. See
    /// `docs/explanation/footprint.md`.
    Bench(bench::BenchArgs),
}

/// Run the conformance harness and print what it found.
///
/// One block per kind, `ok` or `FAIL` per check. Exits non-zero on any
/// failure, so CI and `gmx plugin test` can both read the exit code.
fn run_test_core() -> Result<()> {
    println!("godwinmix test core: 1280x720x30, no outputs, no multiview");
    // Every plugin installed on this machine is checked alongside the built in
    // kinds, because the whole point of the harness is that a plugin is held
    // to the contract the core holds itself to.
    plugin::loader::load_all(&Default::default());
    let mut failures = 0usize;
    let plugins = plugin::harness::check_loaded_plugins(false);
    let built_in = plugin::harness::check_offline_kinds();
    for outcome in built_in.into_iter().chain(plugins) {
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
        anyhow::bail!("{failures} kind(s) failed the harness");
    }
    println!(
        "every built in kind that runs without a network, and every plugin installed, \
         is conformant"
    );
    Ok(())
}

/// Which MCP tool surface a client is shown. The same two names the
/// `[[tokens]]` table uses, so a token's `profile` and this flag mean the
/// same thing.
#[derive(Copy, Clone, Debug, clap::ValueEnum)]
enum McpProfile {
    Standard,
    Minimal,
}

impl From<McpProfile> for godwinmix_protocol::scope::Profile {
    fn from(p: McpProfile) -> Self {
        match p {
            McpProfile::Standard => Self::Standard,
            McpProfile::Minimal => Self::Minimal,
        }
    }
}

/// Parse the command line and do what it says. Both binaries call this.
pub async fn run() -> Result<()> {
    let args = Args::parse();

    core_observe::introspect::begin();
    // Every log line goes to stderr, which keeps stdout clean for the things
    // that are meant to be piped: MCP's protocol, and `gmx dot | dot -Tsvg`.
    // Levels start from `RUST_LOG` and move at runtime from there: see
    // `godwinmix_core::observe::logs`.
    logs::init(logs::Options {
        format: args.log_format.into(),
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
        Some(Command::Bench(b)) => {
            // Needs GStreamer, which the mixer path initialises further down.
            gstreamer::init().context("initialising GStreamer")?;
            return bench::run(b).await;
        }
        Some(Command::Mcp { url, token, profile }) => {
            let url = url.or_else(|| config::env_var("URL")).unwrap_or_else(|| DEFAULT_URL.into());
            let token = token.or_else(|| config::env_var("TOKEN"));
            return mcp::run(&url, token, profile.into()).await;
        }
        Some(Command::Codec { cmd }) => {
            gstreamer::init().context("initialising GStreamer")?;
            let cfg = Config::load(&config::path_in_force(&args.config)).ok();
            return cli::codec::run(cmd, cfg.as_ref(), args.codecs.as_deref());
        }
        Some(Command::Plugin { url, token, cmd }) => {
            let url = url.or_else(|| config::env_var("URL")).unwrap_or_else(|| DEFAULT_URL.into());
            let token = token.or_else(|| config::env_var("TOKEN"));
            return cli::plugin::run(&url, token.as_deref(), cmd).await;
        }
        Some(Command::Chaos { url, token, cmd }) => {
            let url = url.or_else(|| config::env_var("URL")).unwrap_or_else(|| DEFAULT_URL.into());
            let token = token.or_else(|| config::env_var("TOKEN"));
            return cli::chaos::run(&url, token.as_deref(), cmd).await;
        }
        Some(Command::Import { cmd }) => return cli::scene::run_import(cmd),
        Some(Command::Scene { cmd }) => return cli::scene::run_scene(cmd),
        Some(Command::Observe(cmd)) => {
            // These commands print a report to stdout. The mixer's own log on
            // stderr is noise around it unless the operator asked for it.
            if std::env::var_os("RUST_LOG").is_none() {
                logs::set_default_level(logs::LevelCode::WARN);
            }
            return cli::observe::run(cmd).await;
        }
        None => {}
    }

    // Before the config is read and before GStreamer is touched: the
    // protocol is a property of the build, not of this machine, and CI
    // regenerates it on a box with no media stack installed.
    if args.api_info {
        if args.openapi {
            print!("{}", godwinmix_protocol::openapi::json_text(control::openapi()));
        } else if args.markdown {
            print!("{}", godwinmix_protocol::protocol::markdown(control::descriptor()));
        } else {
            print!("{}", godwinmix_protocol::protocol::json_text(control::descriptor()));
        }
        return Ok(());
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
        let _stage = core_observe::introspect::stage("gstreamer init");
        gstreamer::init().context("initialising GStreamer")?;
    }

    if args.test_core {
        return run_test_core();
    }

    if args.probe {
        let cfg = Config::load(&config::path_in_force(&args.config)).ok();
        return cli::codec::print_probe(cfg.as_ref(), args.codecs.as_deref());
    }

    // The LiveboxMix config name is still read when there is no GodwinMix one.
    let config_path = config::path_in_force(&args.config);
    let load = core_observe::introspect::stage("config");
    let cfg = Config::load(&config_path).with_context(|| {
        format!(
            "could not load {}. Run with --example-config to print a starting point.",
            config_path.display()
        )
    })?;
    let bind = args.bind.unwrap_or_else(|| cfg.control.bind.clone());
    let tokens = cfg.tokens(args.rehearsal);
    match tokens.entries().len() {
        0 => info!("control API is open: no token configured"),
        n => info!(tokens = n, "control API requires a token"),
    }
    if args.rehearsal {
        info!("rehearsal core: output.add is refused and only rehearsal tokens are accepted");
    }
    let cfg_media = cfg.media.clone();
    // Where the web UI and any plugin panels are read from.
    ui::configure(cfg.control.ui_dir.as_deref(), cfg.control.plugins_dir.as_deref());
    // The same directory the panels are served from is the one plugins are
    // installed into, so `<plugins_dir>/<name>/<version>/ui/` is both the
    // plugin and its panel and nothing has to be copied anywhere.
    {
        let stage = core_observe::introspect::stage("plugins");
        if let Some(dir) = cfg.control.plugins_dir.as_deref() {
            plugin::loader::set_dir(std::path::PathBuf::from(dir));
        }
        for installed in plugin::loader::load_all(&cfg.plugins) {
            match &installed.problem {
                Some(problem) => warn!(plugin = installed.name(), "{problem}"),
                None => info!(
                    plugin = installed.name(),
                    version = installed.version(),
                    provides = installed.provides.len(),
                    "plugin loaded"
                ),
            }
        }
        drop(stage);
    }
    // Kept for the control plane, which reads the canvas, the snapshot limits,
    // the feature list and the token table off it once at startup.
    let cfg_for_control = cfg.clone();
    drop(load);

    let build = core_observe::introspect::stage("mixer build");
    let (mut mix, handle, cmd_rx, mut bus_rx) = mixer::Mixer::build(cfg)?;
    mix.persist_runtime_to(Config::runtime_store_path(&config_path));
    drop(build);

    // Logs to files, the session log, and the task that records every event.
    // After `Mixer::build`, because the recorder needs a broadcast to join, and
    // before `mix.start`, because that is where the configured sources are
    // built and their lines are exactly the ones somebody debugging a box that
    // will not come up wants in the file.
    let observe_options = core_observe::Options {
        config_path: config_path.clone(),
        startup_report: args.startup_report,
    };
    match core_observe::start(&handle, &observe_options) {
        Ok(dir) => {
            info!(runtime_dir = %dir.display(), "logs and the session log are here");
            // Where a plugin instance's sockets and FIFOs go. Under the runtime
            // directory so that `plugin.add` then `plugin.remove` leaves
            // nothing behind anywhere else.
            plugin::loader::set_runtime_dir(dir);
        }
        Err(e) => warn!(?e, "no runtime directory, so logs stay on stderr only"),
    }

    {
        let _stage = core_observe::introspect::stage("mixer start");
        mix.start().context("starting mixer")?;
    }

    let multiview = mix.multiview_handle();

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
    let state = control::AppState::new(
        &cfg_for_control,
        handle.clone(),
        multiview,
        library,
        converter,
        quit.clone(),
        args.rehearsal,
    );
    let server = tokio::spawn(async move {
        if let Err(e) = control::serve(&bind, state).await {
            error!(?e, "control server stopped");
        }
    });

    if args.startup_report {
        // stdout, because it is a report somebody asked for, not a log line.
        print!("{}", core_observe::introspect::format_startup_report(&core_observe::startup_report()));
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
