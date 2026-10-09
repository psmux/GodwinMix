//! Starting the station, and stopping it with every show.

use super::host::Linked;
use super::registry::Registry;
use super::state::{Launch, Station};
use super::{link, programme, server, supervise};
use anyhow::{Context, Result};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::MixerHandle;
use godwinmix_core::observe as core_observe;
use godwinmix_core::plugin;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::{info, warn};

pub struct Options {
    /// The config in force: the first show's, run in place.
    pub config: PathBuf,
    pub bind: Option<String>,
    pub rehearsal: bool,
    pub codecs: Option<PathBuf>,
    /// Something starts this process again when it exits asking to be.
    pub supervised: bool,
    /// Flags every show is started with as well.
    pub common: Vec<String>,
}

pub async fn run(opts: Options) -> Result<()> {
    let cfg = Config::load(&opts.config).with_context(|| format!("could not load {}", opts.config.display()))?;
    let bind = opts.bind.clone().unwrap_or_else(|| cfg.control.bind.clone());
    // The station owns the control port, so it answers `network.share`.
    crate::control::methods::lifecycle::set_supervised(opts.supervised);
    crate::control::methods::network::configure(&bind, opts.bind.as_ref().is_some_and(|b| *b != cfg.control.bind), &opts.config);
    // The station keeps the machine's device tokens and answers `token.*`;
    // every show reads the same file (see `child::command`).
    let tokens = Arc::new(cfg.tokens(opts.rehearsal).with_devices(crate::devices::for_station(&opts.config)));
    crate::ui::configure(cfg.control.ui_dir.as_deref(), cfg.control.plugins_dir.as_deref());
    let _ = godwinmix_core::catalogue::init(Some(&cfg), opts.codecs.as_deref());
    let runtime = core_observe::runtime_dir(&opts.config);
    load_plugins(&cfg, &tokens, &runtime);

    let (events, commands) = MixerHandle::detached();
    tokio::spawn(drain(commands));
    let render = godwinmix_core::render::Station::start(
        cfg.governor.clone(),
        &godwinmix_core::catalogue::global(),
        cfg.hardware.encode,
        &runtime,
    );
    if let Some(apart) = super::calibrate::apart(&opts.config, opts.codecs.as_deref()) {
        render.measure_apart_with(apart);
    }
    let exe = std::env::current_exe().context("finding this program, to start the shows with")?;
    let launch = Launch { exe, common: opts.common.clone(), calibration: Some(runtime.clone()) };
    let st = Station::new(Registry::open(&opts.config)?, events.clone(), tokens, render.clone(), launch);
    super::direct::inputs::seal_written(&st);
    let addr = link::listen(Arc::new(Linked(st.clone()))).await.context("opening the show link")?;
    let _ = st.link.set(addr);

    // Before any show starts, so each one is told what the port answers
    // HTTPS with (`child::command`).
    let tls = crate::tls::start(&cfg, &bind, &opts.config, &events);
    let ingest = open_channels(&st, &cfg, &opts.config, events);
    render.begin();
    let starting: Vec<String> = st.registry.lock().records.iter().filter(|r| !r.stopped && r.compositing).map(|r| r.id.clone()).collect();
    for id in &starting {
        supervise::start(&st, id);
    }

    let listener = tokio::net::TcpListener::bind(&bind).await.with_context(|| format!("binding the control port {bind}"))?;
    info!(%bind, shows = starting.len(), "station listening");
    eprintln!("GodwinMix is running. Open http://{}/ in a browser.", bind.replacen("0.0.0.0:", "127.0.0.1:", 1));
    if let Some(tls) = &tls {
        crate::tls::announce(&tls.info);
    }
    let app = server::router(st.clone());
    let serving = tokio::spawn(crate::tls::serve(listener, app, tls.map(|t| t.acceptor)));

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = ended() => {}
        _ = st.quit.notified() => {}
    }
    info!("station shutting down; stopping every show");
    st.stopping.store(true, Ordering::SeqCst);
    render.stop_calibrating();
    let ids = st.registry.lock().ids();
    futures_util::future::join_all(ids.iter().map(|id| supervise::stop(&st, id))).await;
    let _ = tokio::task::spawn_blocking(move || ingest.shutdown()).await;
    serving.abort();
    Ok(())
}

/// SIGTERM (a service manager) or SIGHUP (the terminal closed): stop every
/// show as Ctrl-C does, rather than die and leave them running. A station
/// killed outright closes its link, and each show then ends itself
/// (`show::orphaned_watchdog`).
async fn ended() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let (Ok(mut term), Ok(mut hup)) = (signal(SignalKind::terminate()), signal(SignalKind::hangup())) else {
            return std::future::pending().await;
        };
        tokio::select! {
            _ = term.recv() => info!("SIGTERM"),
            _ = hup.recv() => info!("SIGHUP"),
        }
    }
    #[cfg(not(unix))]
    std::future::pending::<()>().await
}

/// The plugins, so the ingest plugin can be started. Only the ingest plugin
/// runs here; every other one runs in the shows, as it did before.
fn load_plugins(cfg: &Config, tokens: &Arc<godwinmix_protocol::scope::Tokens>, runtime: &std::path::Path) {
    if let Some(dir) = cfg.control.plugins_dir.as_deref() {
        plugin::loader::set_dir(godwinmix_host::home::expand(dir));
    }
    super::ingest::configure(cfg.plugins.settings.clone());
    for installed in plugin::loader::load_all(&cfg.plugins) {
        if let Some(problem) = &installed.problem {
            warn!(plugin = installed.name(), "{problem}");
        }
    }
    let minting = tokens.clone();
    plugin::loader::set_token_minter(Box::new(move |plugin, instance| minting.mint_for_plugin(plugin, instance, None)));
    plugin::loader::set_runtime_dir(runtime.join("station"));
}

/// The channels and the ingest plugin that serves them, once for the machine.
fn open_channels(st: &Arc<Station>, cfg: &Config, config: &std::path::Path, events: MixerHandle) -> Arc<plugin::supervisor::Supervisor> {
    let supervisor = plugin::supervisor::Supervisor::new(godwinmix_core::caps::CanvasCaps::new(&cfg.canvas), cfg.plugins.settings.clone());
    let target = Arc::new(programme::FirstShow { station: Arc::downgrade(st), runtime: tokio::runtime::Handle::current() });
    let channels = crate::channels::Channels::open(
        Some(Config::runtime_store_path(config)),
        crate::channels::Ports::from_config(cfg),
        supervisor.clone(),
        events,
        target,
        crate::control::methods::plugins::secrets(),
    );
    channels.use_governor(st.render.governor().clone());
    let _ = st.channels.set(channels);
    super::direct::Direct::attach(st, supervisor.clone());
    super::direct::hls::channel::Links::attach(st);
    let (starting, station) = (supervisor.clone(), st.clone());
    std::thread::spawn(move || {
        if plugin::loader::get(crate::channels::PLUGIN).is_none() {
            return info!("the ingest plugin is not installed; channels take no publishers until it is, and it starts the moment it is");
        }
        if let Err(e) = super::ingest::start(&station, &starting) {
            warn!(error = %format!("{e:#}"), "the ingest plugin would not start");
        }
    });
    supervisor.spawn_pump();
    supervisor
}

/// The channels send the mixer nothing under a station (their target is the
/// first show), so whatever arrives here is logged and dropped.
async fn drain(mut commands: tokio::sync::mpsc::Receiver<godwinmix_core::mixer::Command>) {
    while commands.recv().await.is_some() {
        tracing::debug!("the station has no mixer; a command for one was dropped");
    }
}
