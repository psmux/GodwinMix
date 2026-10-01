//! A show's side of running under a station.
//!
//! `godwinmix --show <id> --station <link>` is today's core with four
//! differences, each made here or asked for from here:
//!
//! * it binds the loopback port it is given (`127.0.0.1:0`) and tells the
//!   station which port that turned out to be, in its hello;
//! * its channels are the station's: it opens no channel file, starts no
//!   ingest plugin and makes no default channel;
//! * its governor asks the station's for everything (`governor.admit`), and
//!   says when something starts or stops going out;
//! * it accepts its link secret as an admin token when it has tokens at all,
//!   so the station can ask it things for the channels and for `show.list`.
//!
//! When the link closes the show stops, because nothing can reach it.

use super::link::client::Link;
use super::link::{Hello, SECRET_ENV};
use godwinmix_protocol::scope::{Token, Tokens};
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone)]
pub struct ShowMode {
    pub id: String,
    pub station: SocketAddr,
    pub secret: String,
}

static MODE: OnceLock<ShowMode> = OnceLock::new();

/// Called once from `run` when the flags say this process is a show.
pub fn enter(id: &str, station: SocketAddr) {
    let secret = std::env::var(SECRET_ENV).unwrap_or_default();
    // Not passed on to anything this show starts: a plugin has no business
    // holding the show's own admin credential.
    std::env::remove_var(SECRET_ENV);
    let _ = MODE.set(ShowMode { id: id.to_string(), station, secret });
}

pub fn mode() -> Option<&'static ShowMode> {
    MODE.get()
}

/// Whether this process is a show under a station.
pub fn under_station() -> bool {
    MODE.get().is_some()
}

/// The show's tokens, with the station's credential added when there are any
/// to add it to. An open core stays open.
pub fn with_station_token(tokens: Tokens) -> Tokens {
    let Some(mode) = MODE.get().filter(|m| !m.secret.is_empty()) else { return tokens };
    if tokens.is_open() {
        return tokens;
    }
    let mut entries = tokens.entries().to_vec();
    let station = Token { id: "station".into(), rehearsal: tokens.rehearsal_core, ..Token::legacy(&mode.secret) };
    entries.push(station);
    Tokens::new(entries, tokens.rehearsal_core)
}

/// How long a show whose station went gets to stop in order before it is
/// stopped outright.
pub const ORPHAN_GRACE: std::time::Duration = std::time::Duration::from_secs(4);

/// The station is gone (stopped, or killed, which closes the link just the
/// same). Nothing can reach this show and nothing will stop it, so it must
/// not outlive its station: it stops in order if it can, and after
/// [`ORPHAN_GRACE`] it ends itself and its process group, which takes the
/// plugins that did not lead groups of their own. Those that do (sources)
/// read their stdin, which closes with this process.
fn orphaned_watchdog() {
    let _ = std::thread::Builder::new().name("orphaned".into()).spawn(|| {
        std::thread::sleep(ORPHAN_GRACE);
        tracing::warn!("the station is gone and this show did not stop in time; ending it");
        #[cfg(unix)]
        unsafe {
            // This show leads its group (the station started it so).
            libc::kill(-(std::process::id() as i32), libc::SIGKILL);
        }
        std::process::exit(1);
    });
}

/// Open the link, point the governor at the station, and say hello with the
/// address this show's control socket is bound to.
pub fn link(bound: SocketAddr, render: &godwinmix_core::render::Station, quit: Arc<tokio::sync::Notify>) -> anyhow::Result<Arc<Link>> {
    let mode = MODE.get().ok_or_else(|| anyhow::anyhow!("not a show under a station"))?;
    let hello = Hello { show: mode.id.clone(), addr: bound, secret: mode.secret.clone(), pid: std::process::id() };
    let lost = Box::new(move || {
        quit.notify_one();
        orphaned_watchdog();
    });
    let link = Link::connect(mode.station, &hello, lost)
        .map_err(|e| anyhow::anyhow!("could not reach the station at {}: {e}", mode.station))?;
    render.governor().set_remote(link.clone());
    let told = link.clone();
    render.on_air_changes(Box::new(move |on| told.on_air(on)));
    if render.on_air() {
        link.on_air(true);
    }
    Ok(link)
}
