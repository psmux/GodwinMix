//! Pieces that live outside the mixer's own program, and the words a person
//! is told about them.
//!
//! A web page is drawn by the browser renderer, a separate program. A camera,
//! a screen, a microphone and a channel each need a first party plugin. A
//! packaged mixer carries all of them where `web` and `plugin::first_party`
//! look. A mixer run from a source checkout has the sources and builds or
//! installs each piece the first time somebody asks for it.
//!
//! The engine does not build anything itself: a build is a download and a
//! compiler, which is the binary's work (`godwinmix::setup`). What is here:
//!
//! ```text
//!   web.rs      where the browser renderer is, and whether a checkout can build it
//!   names.rs    what each piece is called in a person's words
//!   plain.rs    the refusals a person reads, with the detail kept in `data`
//!   system.rs   what only the operating system can supply, and its one command
//!   starter.rs  the hook the binary registers, so the engine can ask for a piece
//! ```
//!
//! The rule for every sentence in `plain.rs` and `system.rs`: say what does
//! not work yet and what happens next or what to press. Program names, file
//! names, settings and element names go in the detail, never the sentence.

pub mod names;
pub mod plain;
pub mod starter;
pub mod system;
pub mod web;

#[cfg(test)]
mod tests;

/// The piece an error waits on, when it is a refusal that a set up fixes:
/// its button is `setup`. The source asked for is then kept and started by
/// itself once that piece is ready.
pub fn waits_on(err: &anyhow::Error) -> Option<String> {
    let action = godwinmix_protocol::ErrorAction::find(err.as_ref())?;
    match action.kind {
        godwinmix_protocol::ActionKind::Setup => action.piece,
        _ => None,
    }
}
