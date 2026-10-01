//! One show as the list keeps it.

use super::OutputRecord;
use godwinmix_protocol::shows::{AlarmSettings, InputSpec};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub name: String,
    /// Where its config is. None for `main`, which is always the config the
    /// station was started with, wherever that is today.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<PathBuf>,
    /// A person stopped it; the station does not start it.
    #[serde(default)]
    pub stopped: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub compositing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputRecord>,
    /// What a person set for its alarms, when they set anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alarms: Option<AlarmSettings>,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

impl Record {
    /// A show that composites, as every show was before wave 4.
    pub fn new(id: &str, name: &str, config: Option<PathBuf>) -> Record {
        Record { id: id.into(), name: name.into(), config, stopped: false, compositing: true, input: None, outputs: Vec::new(), alarms: None }
    }
}
