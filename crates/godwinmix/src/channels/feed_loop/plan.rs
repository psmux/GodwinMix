//! What one show is asked so its outputs match the channels its programme
//! feeds: an `output.add` for a feed with no output yet, an `output.set` for
//! one whose address moved (a new port, a new key), and an `output.remove`
//! for a `to-channel-` output no channel wants any more. Nothing else of the
//! show's is touched.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::super::feed::{ProgrammeFeed, OUTPUT_PREFIX};

/// One call to make of one show.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub method: &'static str,
    pub params: Value,
    /// The channel it is for, and the address it gives, when it gives one.
    pub feed: Option<(String, String)>,
}

/// The steps for `show`, given the feeds, the show's output ids now, and the
/// address each was last given here (`None` when this process has not set
/// it yet, after a restart: it is set once more, to be sure).
pub fn steps(show: &str, first: &str, feeds: &[ProgrammeFeed], outputs: &[String], given: &HashMap<String, String>) -> Vec<Step> {
    let here: Vec<&ProgrammeFeed> = feeds.iter().filter(|f| f.show.as_deref().unwrap_or(first) == show).collect();
    let mut out = Vec::new();
    for f in &here {
        let feed = Some((f.channel.clone(), f.uri.clone()));
        if !outputs.contains(&f.output) {
            out.push(Step { method: "output.add", params: json!({"id": f.output, "uri": f.uri, "policy": "own"}), feed });
        } else if given.get(&f.output) != Some(&f.uri) {
            out.push(Step { method: "output.set", params: json!({"id": f.output, "uri": f.uri}), feed });
        }
    }
    for o in outputs.iter().filter(|o| o.starts_with(OUTPUT_PREFIX) && !here.iter().any(|f| &f.output == *o)) {
        out.push(Step { method: "output.remove", params: json!({"id": o}), feed: None });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(channel: &str, show: Option<&str>, uri: &str) -> ProgrammeFeed {
        ProgrammeFeed { channel: channel.into(), show: show.map(Into::into), output: format!("to-channel-{channel}"), uri: uri.into() }
    }

    #[test]
    fn a_new_feed_is_added_to_the_first_show_with_no_rendition() {
        let s = steps("main", "main", &[feed("live", None, "rtmp://127.0.0.1:1935/live/programme?psk=k")], &["youtube".into()], &HashMap::new());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].method, "output.add");
        assert_eq!(s[0].params, json!({"id": "to-channel-live", "uri": "rtmp://127.0.0.1:1935/live/programme?psk=k", "policy": "own"}));
        assert!(s[0].params.get("rendition").is_none(), "the programme encoder, nothing encoded again");
    }

    #[test]
    fn one_in_place_is_left_alone_set_once_after_a_restart_and_never_doubled() {
        let f = [feed("live", Some("main"), "rtmp://a")];
        let outputs = vec!["to-channel-live".to_string()];
        let after_restart = steps("main", "main", &f, &outputs, &HashMap::new());
        assert_eq!(after_restart.iter().map(|s| s.method).collect::<Vec<_>>(), vec!["output.set"], "kept by the show, given its address once");
        let given = HashMap::from([("to-channel-live".to_string(), "rtmp://a".to_string())]);
        assert!(steps("main", "main", &f, &outputs, &given).is_empty());
        let moved = [feed("live", Some("main"), "rtmp://b")];
        assert_eq!(steps("main", "main", &moved, &outputs, &given)[0].method, "output.set");
    }

    #[test]
    fn a_feed_that_went_elsewhere_is_taken_off_and_other_outputs_stay() {
        let outputs = vec!["to-channel-live".to_string(), "to-channel-hall".to_string(), "youtube".to_string()];
        let s = steps("main", "main", &[feed("hall", None, "rtmp://h"), feed("live", Some("second"), "rtmp://l")], &outputs, &HashMap::new());
        let removed: Vec<&Value> = s.iter().filter(|s| s.method == "output.remove").map(|s| &s.params["id"]).collect();
        assert_eq!(removed, vec!["to-channel-live"]);
        assert!(!s.iter().any(|s| s.params["id"] == "youtube"));
    }
}
