//! A show's input passphrases, sealed and unsealed.
//!
//! An SRT passphrase can be written in the input's address
//! (`srt://host:9000?passphrase=...`) or in its `params`, for the main input
//! and for the backup. Each is sealed in the secret store under
//! `show.<id>.input`, as an output's key is, and the list keeps the secret
//! store's sentinel `__secret__` where it was. So `show.list` and the file
//! on disk never hold the passphrase, and an input sent back unchanged (the
//! sentinel in place) keeps the one that was sealed. It is unsealed only to
//! build the direct host's table.

mod place;
#[cfg(test)]
mod tests;

use crate::station::state::Station;
use godwinmix_core::secrets::SENTINEL;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::InputSpec;
use place::{read, write, PLACES};
use tracing::{info, warn};

/// Where a show's input passphrases are sealed.
pub fn scope(show: &str) -> String {
    format!("show.{show}.input")
}

fn secrets() -> &'static godwinmix_core::secrets::Secrets {
    crate::control::methods::plugins::secrets()
}

/// Seal every passphrase `input` carries and answer what the list keeps: the
/// sentinel where each was. A sentinel sent back keeps what was sealed; a
/// place left empty forgets it.
pub fn seal(show: &str, mut input: InputSpec) -> Result<InputSpec, RpcError> {
    let scope = scope(show);
    for (slot, backup, in_uri) in PLACES {
        match read(&input, backup, in_uri) {
            Some(v) if v == SENTINEL => {}
            Some(v) => {
                secrets().set(&scope, slot, &v).map_err(|e| RpcError::internal(format!("sealing the input's passphrase: {e:#}")))?;
                write(&mut input, backup, in_uri, Some(SENTINEL));
            }
            None => {
                if secrets().has(&scope, slot) {
                    let _ = secrets().set(&scope, slot, "");
                }
            }
        }
    }
    Ok(input)
}

/// `seal` for an input that may not be there.
pub fn seal_some(show: &str, input: Option<InputSpec>) -> Result<Option<InputSpec>, RpcError> {
    input.map(|i| seal(show, i)).transpose()
}

/// The input as the host opens it: each sentinel replaced by what was
/// sealed, or taken out when nothing was.
pub fn unsealed(show: &str, mut input: InputSpec) -> InputSpec {
    let scope = scope(show);
    for (slot, backup, in_uri) in PLACES {
        if read(&input, backup, in_uri).as_deref() == Some(SENTINEL) {
            let value = secrets().get(&scope, slot);
            write(&mut input, backup, in_uri, value.as_deref());
        }
    }
    input
}

/// Whether the input still carries a passphrase in the clear, as a list
/// written before they were sealed does.
pub fn in_clear(input: &InputSpec) -> bool {
    PLACES.iter().any(|&(_, backup, in_uri)| read(input, backup, in_uri).is_some_and(|v| v != SENTINEL))
}

/// Seal what a list written before this kept in the clear, once, at start.
pub fn seal_written(st: &Station) {
    let mut reg = st.registry.lock();
    let mut moved = 0;
    for r in reg.records.iter_mut() {
        let Some(input) = r.input.clone().filter(in_clear) else { continue };
        match seal(&r.id, input) {
            Ok(sealed) => {
                r.input = Some(sealed);
                moved += 1;
            }
            Err(e) => warn!(show = %r.id, error = %e.message, "an input passphrase stays in the list of shows, because it could not be sealed"),
        }
    }
    if moved > 0 {
        match reg.save() {
            Ok(()) => info!(shows = moved, "input passphrases moved from the list of shows into the secret store"),
            Err(e) => warn!(error = %e, "the list of shows could not be written with its passphrases sealed"),
        }
    }
}

/// Forget the sealed passphrases of a show that went.
pub fn forget(show: &str) {
    secrets().forget(&scope(show));
}

/// Forget everything sealed for a show: its input's passphrases and its
/// outputs' addresses and keys.
pub fn forget_show(show: &str) {
    forget(show);
    super::outputs::forget(show);
}
