//! The direct table, as the station hands it over under `direct` in the
//! settings (`dev/plans/wave4-direct-table.md`): one row per show whose
//! input this host opens.

use serde_json::Value;

use super::input::spec::InputSpec;
use crate::sends::{destination, Wanted};
use crate::transcode::{stream_specs, StreamSpec};

/// The one stream a direct show publishes on the hub.
pub const STREAM: &str = "main";

/// The hub application a show's input is published under. A dot is outside
/// what a channel slug may hold, so no channel can ever take it.
pub fn app_of(show: &str) -> String {
    format!("direct.{show}")
}

/// One show, as the table asks for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub input: InputSpec,
    /// Each a channel destination row, read as one.
    pub outputs: Vec<Wanted>,
    /// What to build for the outputs that convert.
    pub transcode: Vec<StreamSpec>,
    /// `{alarms, pictures, thresholds?}`, handed to the vitals as it is.
    pub monitor: Value,
    /// Why the input cannot be opened as written, when it cannot. The show
    /// still runs, idle, and says so in `direct.input`.
    pub refused: Option<String>,
}

impl Row {
    pub fn app(&self) -> String {
        app_of(&self.id)
    }
}

/// Every row in `params.direct`, and a sentence for each row that could
/// not be read.
pub fn rows(params: &Value) -> (Vec<Row>, Vec<String>) {
    let (mut out, mut refused) = (Vec::new(), Vec::new());
    for r in params.get("direct").and_then(Value::as_array).into_iter().flatten() {
        match row(r) {
            Ok(row) if out.iter().any(|o: &Row| o.id == row.id) => refused.push(format!("show {} is in the table twice; the first row is used", row.id)),
            Ok(row) => out.push(row),
            Err(why) => refused.push(why),
        }
    }
    (out, refused)
}

fn row(r: &Value) -> Result<Row, String> {
    let id = r.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
    if id.is_empty() {
        return Err("a direct table row has no id, so it was left out".into());
    }
    let raw = r.get("input").unwrap_or(&Value::Null);
    let (input, refused) = match InputSpec::from_json(raw) {
        Ok(input) => (input, None),
        Err(e) => {
            let uri = raw.get("uri").and_then(Value::as_str).unwrap_or_default();
            (InputSpec { uri: uri.to_string(), program: None, params: Value::Null, backup: None }, Some(e.message))
        }
    };
    let app = app_of(&id);
    let outputs = r
        .get("outputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|d| destination(&id, &app, d))
        .map(|mut w| {
            w.stream = STREAM.into();
            w
        })
        .collect();
    Ok(Row {
        name: r.get("name").and_then(Value::as_str).unwrap_or(&id).to_string(),
        input,
        outputs,
        transcode: stream_specs(&app, r.get("transcode")),
        monitor: r.get("monitor").cloned().unwrap_or(Value::Null),
        refused,
        id,
    })
}
