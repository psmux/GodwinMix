//! Headend mode: GodwinMix in place of Flussonic.
//!
//! `godwinmix headend --flussonic /etc/flussonic/flussonic.conf` starts a
//! station with mixing off, makes each `stream` of that file a direct show,
//! and answers Flussonic's URLs and API on the file's `http` port, so the
//! gateway, the caches and the monitoring page in front of it need no change.
//! The file is watched; an edit restarts only the streams it changed.
//!
//! `godwinmix headend --port 8090` with no file is a headend set up on its
//! own screen: the channels the station keeps (`native/`), served on the
//! same URLs. Every station keeps such channels, headend mode or not.
//!
//! ```text
//!   flussonic.conf ──watch──► import ──show.add_many──► station (direct shows)
//!                                                          │
//!   :8090  /<name>/index.m3u8, tracks-*/..  ◄── Segments ──┤ (HLS of each show)
//!          /streamer/api/v3/streams         ◄── live ──────┘ (what each show reports)
//! ```
//!
//! The import, the URL shapes and the JSON are `godwinmix_flussonic`'s;
//! this module holds them to a running station.

mod api;
mod apply;
mod bridge;
mod bridge_parse;
pub mod calls;
mod facade;
mod import_read;
mod ingest;
mod keep;
mod live;
mod live_tracks;
mod media;
pub mod methods;
pub mod native;
mod picture;
mod report;
mod saved;
mod state;
mod ts_bridge;
mod watch;

pub use state::Headend;

use anyhow::{Context, Result};
use godwinmix_core::config::Config;
use std::path::PathBuf;

/// `godwinmix headend`.
#[derive(clap::Args, Debug, Clone)]
pub struct Args {
    /// Flussonic's config to follow. Left out: `flussonic` in the
    /// `[headend]` table of the GodwinMix config, or none: the channels are
    /// then set up on the headend screen.
    #[arg(long, value_name = "FILE")]
    pub flussonic: Option<PathBuf>,
    /// Answer on this port instead of the file's `http` port, to run beside
    /// Flussonic while testing.
    #[arg(long)]
    pub port: Option<u16>,
    /// Answer on this address instead of the one the `http` line names.
    #[arg(long, value_name = "ADDR")]
    pub address: Option<String>,
    /// Write Flussonic style log lines here, for readers that count them.
    /// Left out: `flussonic_log` in `[headend]`, or no such log.
    #[arg(long, value_name = "FILE")]
    pub flussonic_log: Option<PathBuf>,
    /// Print what the import would make, and every line it does not carry,
    /// then exit without starting anything.
    #[arg(long)]
    pub check: bool,
}

/// What the headend runs with, from the flags and the `[headend]` table.
#[derive(Debug, Clone)]
pub struct Settings {
    pub file: Option<PathBuf>,
    pub port: Option<u16>,
    pub address: Option<String>,
    pub log: Option<PathBuf>,
    /// The GodwinMix config's plugin trust settings, for installing the
    /// ingest plugin.
    pub allow_unsigned: bool,
    pub marketplaces_only: Vec<String>,
}

fn table_path(cfg: &Config, key: &str) -> Option<PathBuf> {
    cfg.extra.get("headend")?.get(key)?.as_str().map(PathBuf::from)
}

pub fn settings(args: &Args, cfg: Option<&Config>) -> Result<Settings> {
    let from_table = |key: &str| cfg.and_then(|c| table_path(c, key));
    let file = args.flussonic.clone().or_else(|| from_table("flussonic"));
    let port = args.port.or_else(|| cfg.and_then(|c| c.extra.get("headend")?.get("port")?.as_integer()).and_then(|p| u16::try_from(p).ok()));
    Ok(Settings {
        file,
        port,
        address: args.address.clone(),
        log: args.flussonic_log.clone().or_else(|| from_table("flussonic_log")),
        allow_unsigned: cfg.is_none_or(|c| c.plugins.allow_unsigned),
        marketplaces_only: cfg.map(|c| c.marketplaces_only()).unwrap_or_default(),
    })
}

/// `--check`: the import's plan and notes, as text.
pub fn check(s: &Settings) -> Result<String> {
    let file = s.file.as_ref().context("--check reads a Flussonic config: give --flussonic FILE")?;
    let text = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let doc = godwinmix_flussonic::Document::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
    let import = godwinmix_flussonic::import::import(&doc.config);
    let copies = import.shows.iter().filter(|p| p.copy).count();
    let mut out = format!(
        "{}: {} streams, {} converted and {} copied, {} off\n",
        file.display(),
        import.shows.len(),
        import.shows.len() - copies,
        copies,
        import.shows.iter().filter(|p| p.disabled).count()
    );
    for (scope, n) in &import.notes {
        let kind = match n.kind {
            godwinmix_flussonic::import::NoteKind::Ignored => "ignored",
            godwinmix_flussonic::import::NoteKind::NotCarried => "not carried",
        };
        match n.line {
            0 => out.push_str(&format!("{scope}, {kind}: {}\n", n.what)),
            line => out.push_str(&format!("line {line}, {scope}, {kind}: {}\n", n.what)),
        }
    }
    Ok(out)
}

/// What the station calls once its port is bound: start the headend on it.
pub fn ready(s: Settings) -> crate::station::Ready {
    Box::new(move |st| {
        tokio::spawn(async move {
            let station = st.clone();
            if let Err(e) = state::start(st, s).await {
                // A headend that cannot answer is no headend: stop, so the
                // service manager says so.
                tracing::error!(error = %format!("{e:#}"), "headend mode could not start; stopping");
                eprintln!("headend mode could not start: {e:#}");
                station.quit.notify_one();
            }
        });
    })
}
