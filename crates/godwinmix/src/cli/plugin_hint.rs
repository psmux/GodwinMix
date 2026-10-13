//! What `gmx plugin add` says to do next, by what the plugin brought. A
//! source is added as a source, an output as an output, a panel is in the
//! web UI already, and a service is running. Telling somebody who installed
//! a panel to `gmx ctl source add` it sends them down a path that fails.

use serde_json::Value;

/// The lines after `installed <name> v<version>`, one per kind it brought,
/// in the order the provides are listed. `plugin.add` answers with
/// `provides` and, from 0.3.10, `provide_kinds`; a provide with no kind is
/// read by its id, so an older core still gets a sensible line.
pub fn next_steps(record: &Value) -> Vec<String> {
    let kinds = &record["provide_kinds"];
    let provides: Vec<&str> = record["provides"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for id in provides {
        let kind = kinds[id].as_str().unwrap_or_else(|| id.rsplit('/').next().unwrap_or(""));
        if seen.contains(&kind) {
            continue;
        }
        seen.push(kind);
        if let Some(line) = line_for(kind, id) {
            out.push(line);
        }
    }
    out
}

fn line_for(kind: &str, id: &str) -> Option<String> {
    Some(match kind {
        "source" => format!("Add one with:  gmx ctl source add <id> --type {id}"),
        "output" => format!("Add one in the web UI under Outputs, or with output.add {{\"type\": \"{id}\"}}"),
        "filter" => format!("Put it on a source in the web UI, or with filter.add {{\"type\": \"{id}\"}}"),
        "panel" => format!(
            "Its panel, {id}, is in the web UI: View lists it once the page has read the plugin \
             panels again (reload the page, or Reload plugin panels in the command palette)"
        ),
        "service" | "device" | "transition" => format!("{id} is running. gmx plugin list shows it"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_panel_only_plugin_is_told_where_its_panel_is_and_not_to_add_a_source() {
        let record = json!({ "provides": ["hls-player/embed"], "provide_kinds": { "hls-player/embed": "panel" } });
        let lines = next_steps(&record);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("View"), "{lines:?}");
        assert!(!lines[0].contains("source add"), "{lines:?}");
    }

    #[test]
    fn a_source_is_added_and_a_service_is_said_to_run() {
        let record = json!({
            "provides": ["clock/source", "clock/sync", "clock/source2"],
            "provide_kinds": { "clock/source": "source", "clock/sync": "service", "clock/source2": "source" }
        });
        let lines = next_steps(&record);
        assert_eq!(lines.len(), 2, "one line per kind: {lines:?}");
        assert!(lines[0].contains("gmx ctl source add <id> --type clock/source"));
        assert!(lines[1].contains("clock/sync is running"));
    }

    #[test]
    fn an_older_core_with_no_kinds_is_read_by_the_id() {
        let lines = next_steps(&json!({ "provides": ["zero-dep/source"] }));
        assert!(lines[0].contains("source add"), "{lines:?}");
        assert!(next_steps(&json!({ "provides": ["x/embed"] })).is_empty(), "an unknown id says nothing rather than something wrong");
    }
}
