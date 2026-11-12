//! Errors, each saying what is wrong now and what to do next.

use std::fmt;

use crate::session::Link;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A switcher id that is not a slug.
    BadSlug { slug: String },
    /// A switcher with this slug is already connected.
    Exists { slug: String },
    /// No switcher has this slug.
    Unknown { slug: String, known: Vec<String> },
    /// No switcher is connected at all.
    NoSwitchers,
    /// More than one is connected and the call did not say which.
    WhichOne { known: Vec<String> },
    /// The address could not be read or resolved.
    BadAddress { address: String, why: String },
    /// The switcher is not up, so the command was not sent.
    Down { slug: String, link: Link },
    /// The switcher's task has stopped; it was removed.
    Closed { slug: String },
}

impl Error {
    /// A short code a caller can match on.
    pub fn code(&self) -> &'static str {
        match self {
            Error::BadSlug { .. } => "bad_slug",
            Error::Exists { .. } => "exists",
            Error::Unknown { .. } => "unknown_switcher",
            Error::NoSwitchers => "no_switchers",
            Error::WhichOne { .. } => "which_switcher",
            Error::BadAddress { .. } => "bad_address",
            Error::Down { .. } => "switcher_down",
            Error::Closed { .. } => "closed",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadSlug { slug } => write!(
                f,
                "'{slug}' is not a switcher id. Use lower case letters, digits and single \
                 hyphens, starting with a letter, such as atem-main."
            ),
            Error::Exists { slug } => write!(
                f,
                "a switcher called '{slug}' is already connected. Pick another id, or remove \
                 that one first."
            ),
            Error::Unknown { slug, known } => write!(
                f,
                "no switcher is called '{slug}'. Connected: {}. Connect it first, or use one of those.",
                list(known)
            ),
            Error::NoSwitchers => write!(
                f,
                "no switcher is connected. Connect one with its address first, such as \
                 connect {{id: \"atem-main\", address: \"192.0.2.10\"}}."
            ),
            Error::WhichOne { known } => write!(
                f,
                "{} switchers are connected ({}), so say which one with 'switcher'.",
                known.len(),
                list(known)
            ),
            Error::BadAddress { address, why } => write!(
                f,
                "cannot use '{address}' as a switcher address: {why}. Give an IP address or a \
                 host name, with :port only if it is not 9910."
            ),
            Error::Down { slug, link } => write!(
                f,
                "'{slug}' is {}, so the command was not sent. Send it again once the link is up.",
                link.describe()
            ),
            Error::Closed { slug } => write!(
                f,
                "'{slug}' was disconnected while the command was on its way. Connect it again."
            ),
        }
    }
}

fn list(known: &[String]) -> String {
    if known.is_empty() {
        "none".into()
    } else {
        known.join(", ")
    }
}

impl std::error::Error for Error {}
