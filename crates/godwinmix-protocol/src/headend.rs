//! `headend.import` and `headend.status`: a Flussonic config made into
//! shows from the page or an agent, and whether this station follows one.
//!
//! The import is the one `godwinmix headend` runs (`godwinmix-flussonic`):
//! each `stream` a show without compositing, its `input` the show's input,
//! one HLS output with the `transcoder` line as its rendition, and every
//! line it does not carry listed. These types are only its report.

use crate::shows::ShowAddManyResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `headend.import`. One of `text` and `path`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HeadendImportRequest {
    /// The config's text, as pasted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Or a file on the station's machine, such as
    /// `/etc/flussonic/flussonic.conf`, read by the station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Left out: true. Read the config, price its shows and make nothing.
    /// False makes them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

/// One `stream` of the config, as the show it becomes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImportedStream {
    /// Flussonic's name, the show's name when it is made.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `disabled true`: the show is made and stopped.
    pub disabled: bool,
    /// No `transcoder`: the input is served as it comes.
    pub copy: bool,
    /// The show's input, as `show.add` takes it, with any password in an
    /// address shown as `***`.
    pub input: Value,
    /// The show's outputs: the HLS output with its params and rendition,
    /// then one per `push`.
    pub outputs: Vec<Value>,
}

/// A line the import read and does not act on, or acts on differently.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImportNote {
    /// `global`, or `stream NAME`.
    pub scope: String,
    /// 0 when the note is about the whole config.
    pub line: usize,
    /// `ignored`: nothing in GodwinMix uses it. `not_carried`: understood,
    /// and GodwinMix does something else, which `what` says.
    pub kind: String,
    pub what: String,
}

/// `headend.import`'s answer: the report, then what was made.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct HeadendImportReport {
    /// `pasted text`, or the path that was read.
    pub source: String,
    pub dry_run: bool,
    pub streams: Vec<ImportedStream>,
    pub converted: usize,
    pub copied: usize,
    pub disabled: usize,
    pub notes: Vec<ImportNote>,
    /// `show.add_many`'s answer for these shows: on a dry run what would be
    /// made, what it costs and what would be refused; otherwise what was.
    pub shows: ShowAddManyResult,
    /// The Flussonic config this station follows, when it follows one. A
    /// show made by an import is not in that file, and the file's own
    /// streams are not touched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub following: Option<String>,
}

/// `headend.status`: whether this station runs in headend mode.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HeadendStatus {
    /// True while the station follows a Flussonic config
    /// (`godwinmix headend --flussonic FILE`).
    pub following: bool,
    /// The file it follows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Where Flussonic's URLs and API answer, `address:port`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub listeners: Vec<String>,
    /// The Flussonic style log, when one is written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
    /// Streams in the file as last read.
    pub streams: usize,
    /// Of those, the ones running as a show.
    pub shows: usize,
    /// What a reader of this answer can do next, in one sentence.
    pub note: String,
}
