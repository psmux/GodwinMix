//! Making the shows match the followed file. The work is the keeper's
//! (`keep.rs`); what is here is which streams of the file it keeps: every
//! one except those edited on the headend screen, which the station's own
//! channels hold from then on (`native/`).

use super::state::Headend;
use godwinmix_flussonic::import;
use tracing::info;

/// Import the document in hand and apply it. `announce` logs every line
/// the import does not carry, which is for a file just read.
pub async fn apply_file(h: &Headend, announce: bool) {
    let (plans, notes) = {
        let doc = h.doc.lock();
        let i = import::import(&doc.config);
        (i.shows, i.notes)
    };
    for (scope, n) in notes.iter().filter(|_| announce) {
        info!(line = n.line, scope = %scope, kind = ?n.kind, "headend import: {}", n.what);
    }
    let owned = super::native::Native::of(&h.st).owned();
    h.keep.apply(plans.into_iter().filter(|p| !owned.contains(&p.name)).collect()).await;
}
