//! The Graphics gallery: every designed thing the mixer can put on air, in
//! one list, whoever made it.
//!
//! An item is a folder with a `graphic.toml` in it (the format is
//! `docs/reference/gallery-format.md`). Saved items live in the gallery's
//! folder; the template pack, the starter designs compiled into the binary
//! and the SVG templates already in the media library are listed beside
//! them, read only.
//!
//! ```text
//!   gallery.save / import --> draft --> store::write --> <gallery>/<id>/graphic.toml
//!   gallery.list          --> store::list: saved, starters, pack, library templates
//!   gallery.preview       --> preview: drawn on demand, kept in .previews/
//!   gallery.place         --> place: where on the canvas, by zone
//! ```
//!
//! Nothing here runs on the mixer thread or a streaming thread. Every
//! function that reads a disk or draws a picture is blocking and is called
//! from a blocking task by the control server.

pub mod bundle;
pub mod detect;
pub mod draft;
pub mod edit;
pub mod entry;
pub mod manifest;
pub mod place;
pub mod preview;
pub mod starters;
pub mod store;

pub use entry::Entry;
pub use manifest::Manifest;

use std::path::PathBuf;

/// The file that marks the gallery's folder, so the media library, which
/// the folder sits inside by default, does not list its pictures as clips.
pub const MARKER: &str = ".gmx-gallery";

/// The file that makes a folder an item.
pub const MANIFEST: &str = "graphic.toml";

/// The longest id.
pub const MAX_ID: usize = 64;

/// The gallery's folder, as the config says.
pub fn dir() -> PathBuf {
    crate::graphics::brand::gallery()
}

/// The id a name makes: lower case letters and digits, with one dash for
/// every run of anything else, at most [`MAX_ID`] long.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= MAX_ID {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "graphic".into()
    } else {
        out
    }
}

/// True for a string [`slug`] could have made.
pub fn is_slug(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

#[cfg(test)]
mod tests;
