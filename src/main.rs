//! LiveboxMix: a live RTMP video mixer.
//!
//! Multiple RTMP sources come in, one of them is on program at a time, and the
//! program feed goes out to one or more RTMP destinations without ever
//! stopping. Switching source is instant and does not disturb the outgoing
//! stream, because the output encoder is started once and runs for the life of
//! the broadcast; everything that changes happens upstream of it in raw video.
//!
//! See `mixer.rs` for why that arrangement is the whole design.

mod caps;
mod config;
mod control;
mod ctl;
mod gstutil;
mod input;
mod media;
mod mixer;
mod multiview;
mod output;
mod probe;
mod state;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::Config;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

const EXAMPLE_CONFIG: &str = include_str!("../liveboxmix.example.toml");

#[derive(Parser, Debug)]
#[command(name = "liveboxmix", about = "Live RTMP video mixer with hot source switching")]
struct Args {
    /// Path to the TOML configuration file.
    #[arg(short, long, default_value = "liveboxmix.toml")]
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

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Control a running mixer over its HTTP API.
    Ctl {
        /// Address of the mixer's control server.
        #[arg(long, default_value = "http://127.0.0.1:8080", env = "LIVEBOXMIX_URL")]
        url: String,
        #[command(subcommand)]
        cmd: ctl::Ctl,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    // The control subcommand talks to an already-running mixer and needs none
    // of the setup below.
    if let Some(Command::Ctl { url, cmd }) = args.command {
        return ctl::run(&url, cmd).await;
    }

    if args.example_config {
        print!("{EXAMPLE_CONFIG}");
        return Ok(());
    }

    gstreamer::init().context("initialising GStreamer")?;

    if args.probe {
        let b = probe::Backends::probe(config::Accel::Auto, config::Accel::Auto)?;
        println!("video decoder : {}  ({:?})", b.video_decode.element, b.video_decode.accel);
        println!("video encoder : {}  ({:?})", b.video_encode.element, b.video_encode.accel);
        println!("audio decoder : {}", b.audio_decode);
        println!("audio encoder : {}", b.audio_encode);
        return Ok(());
    }

    let cfg = Config::load(&args.config).with_context(|| {
        format!(
            "could not load {}. Run with --example-config to print a starting point.",
            args.config.display()
        )
    })?;
    let bind = args.bind.unwrap_or_else(|| cfg.control.bind.clone());
    let cfg_media = cfg.media.clone();

    let (mut mix, handle, cmd_rx, mut bus_rx) = mixer::Mixer::build(cfg)?;
    mix.persist_runtime_to(Config::runtime_store_path(&args.config));
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
    let state = control::AppState { mixer: handle.clone(), frames, library };
    let server = tokio::spawn(async move {
        if let Err(e) = control::serve(&bind, state).await {
            error!(?e, "control server stopped");
        }
    });

    tokio::signal::ctrl_c().await.ok();
    info!("shutting down");
    let _ = handle.send(mixer::Command::Shutdown);
    server.abort();
    let _ = tokio::task::spawn_blocking(move || mixer_thread.join()).await;
    Ok(())
}
