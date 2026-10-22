//! Watching Flussonic's config. Once a second the file's time and size are
//! looked at; when either moved it is read again and applied as a diff. A
//! file that will not read leaves every show running as it was, and says
//! why. Every 30 s the file is applied again, which makes any stream that
//! could not be made the first time (a refusal, a feed not there yet).

use super::state::Headend;
use godwinmix_flussonic::Document;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tracing::{info, warn};

const LOOK: Duration = Duration::from_secs(1);
const AGAIN: Duration = Duration::from_secs(30);

fn stamp(h: &Headend) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(&h.file).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// Read the file into the headend. False when it would not read.
pub fn reread(h: &Headend) -> bool {
    let text = match std::fs::read_to_string(&h.file) {
        Ok(t) => t,
        Err(e) => {
            warn!(file = %h.file.display(), error = %e, "headend: the config would not read; the streams keep running as they are");
            return false;
        }
    };
    match Document::parse(&text) {
        Ok(doc) => {
            *h.doc.lock() = doc;
            true
        }
        Err(e) => {
            warn!(file = %h.file.display(), error = %e, "headend: the config has a mistake; the streams keep running as they are until it is fixed");
            false
        }
    }
}

pub async fn watch(h: Arc<Headend>) {
    let mut seen = stamp(&h);
    let mut applied = Instant::now();
    loop {
        tokio::select! {
            _ = tokio::time::sleep(LOOK) => {}
            _ = h.changed.notified() => {}
        }
        let now = stamp(&h);
        if now != seen {
            seen = now;
            info!(file = %h.file.display(), "headend: the config changed");
            if reread(&h) {
                super::apply::apply_file(&h).await;
                applied = Instant::now();
            }
        } else if applied.elapsed() >= AGAIN {
            super::apply::apply_file(&h).await;
            applied = Instant::now();
        }
    }
}
