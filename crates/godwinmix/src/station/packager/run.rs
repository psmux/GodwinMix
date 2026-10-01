//! The packager process: bind a loopback port, say which on stdout, and
//! serve until the station goes.

use super::outputs::Outputs;
use super::serve::{self, PackagerDoor};
use super::wire::{LISTENING, SECRET_ENV};
use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};

pub async fn run() -> Result<()> {
    let secret = std::env::var(SECRET_ENV).ok().filter(|s| !s.is_empty()).with_context(|| {
        format!("the HLS packager is started by a station, which sets {SECRET_ENV}; start godwinmix without --hls-packager")
    })?;
    end_with_the_station();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.context("binding a loopback port for the HLS packager")?;
    let addr = listener.local_addr()?;
    let app = serve::router(PackagerDoor::new(Arc::new(Outputs::default()), &secret));
    {
        let mut out = std::io::stdout().lock();
        writeln!(out, "{LISTENING} {addr}")?;
        out.flush()?;
    }
    info!(%addr, "the HLS packager is listening");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}

/// The station holds the other end of stdin. When it reads to the end the
/// station has gone, however it went, and so does this process: a packager
/// is never left running on its own.
fn end_with_the_station() {
    let watching = std::thread::Builder::new().name("gmx-packager-stdin".into()).spawn(|| {
        let mut sink = [0u8; 64];
        while matches!(std::io::stdin().read(&mut sink), Ok(n) if n > 0) {}
        info!("the station has gone; the HLS packager stops");
        std::process::exit(0);
    });
    if let Err(e) = watching {
        warn!(%e, "no thread to watch for the station going; this packager stops when the station stops it");
    }
}
