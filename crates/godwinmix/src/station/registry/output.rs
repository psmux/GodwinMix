//! An output of a show without compositing, as the list of shows keeps it.
//!
//! Like a channel's destination: the record says what a list shows, and the
//! address and the key are sealed in the secret store under
//! `show.<id>.output` (`crate::station::direct::outputs`). A show that
//! composites keeps its outputs inside its own process; the ones it took
//! over when compositing was turned on stay here too, parked, so turning it
//! off again can hand them back with their keys.

use godwinmix_protocol::rendition::RenditionChoice;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputRecord {
    pub id: String,
    pub platform: String,
    pub label: String,
    /// Scheme, host and port, for a person reading the file.
    pub uri_host: String,
    #[serde(default)]
    pub has_key: bool,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
}
