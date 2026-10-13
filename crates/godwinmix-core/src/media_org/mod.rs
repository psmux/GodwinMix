//! Organising the media library: real folders on disk, and free tags kept
//! in a small file beside the clips.
//!
//! A folder is a directory under the library's own, any depth, so a folder
//! made in a file manager simply shows up and a file moved by hand is where
//! the next listing says it is. Moving a file or a folder is a rename on the
//! same disk. Nothing here deletes a media file: removing a folder moves
//! what is in it out first and then removes the empty directory.
//!
//! Tags have no file system home that works on Windows, macOS and Linux
//! alike, so they live in `.godwinmix-media.json`, keyed by the path
//! relative to the library. See `tags`.

mod filter;
mod folders;
mod moves;
mod paths;
mod remove;
mod tag_rules;
mod tags;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_folders;

pub use filter::{folder_counts, tag_counts, Filter};
pub use folders::{list_folders, make_folder, move_folder, plan_folder, Contents};
pub use remove::remove_folder;
pub use moves::{move_file, plan_file, Moved};
pub use paths::{inside, join, parent_of, plain, rel_path, segment};
pub use tag_rules::{tag_files, tag_list};
pub use tags::{forget, load, moved, update, Tags, DAMAGED, FILE};

/// Why a change was refused, with what a caller needs to put it right.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    /// What went wrong and what to do next, in one or two sentences.
    pub message: String,
    /// The param at fault: `name`, `folder`, `to`, `tags`.
    pub field: &'static str,
    /// What is there instead, when that helps: the folders, the files in the
    /// way.
    pub valid: Vec<String>,
    /// True when the thing named is not there, which callers answer as not
    /// found rather than as bad params.
    pub missing: bool,
}

impl Refusal {
    pub fn invalid(field: &'static str, message: impl Into<String>) -> Self {
        Self { message: message.into(), field, valid: Vec::new(), missing: false }
    }

    pub fn missing(field: &'static str, message: impl Into<String>) -> Self {
        Self { missing: true, ..Self::invalid(field, message) }
    }

    pub fn with_valid(mut self, valid: Vec<String>) -> Self {
        self.valid = valid;
        self
    }

    pub fn io(what: &str, e: std::io::Error) -> Self {
        Self::invalid(
            "name",
            format!("{what} failed: {e}. Check the media folder can be written to and nothing has the file open, then try again."),
        )
    }
}
