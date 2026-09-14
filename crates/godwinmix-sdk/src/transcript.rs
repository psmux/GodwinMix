//! Recorded transcripts, for `gmx plugin test --offline`.
//!
//! A transcript is a JSONL file. Each line is one object with exactly one key:
//!
//! ```jsonl
//! {"core":   {"jsonrpc":"2.0","id":0,"result":{"core":"godwinmix", ...}}}
//! {"plugin": {"method":"initialized"}}
//! ```
//!
//! `core` lines are written to the plugin's stdin. `plugin` lines are matched
//! against what comes back on stderr, as a subset: the line must contain the
//! keys given, with those values, and may contain anything else. `"*"` as a
//! value matches whatever is there, which is how a timestamp or a generated id
//! is allowed to vary.
//!
//! Bytes in, bytes out, no sockets, no clock. A plugin's CI runs this on any
//! runner in seconds, which is the promise in 09 section 4 item 9.

use serde_json::Value;

/// One line of a transcript.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Write this to the plugin's stdin.
    Core(Value),
    /// Expect this, as a subset, on the plugin's stderr.
    Plugin(Value),
    /// A comment or a blank line, kept so line numbers line up in errors.
    Ignored,
}

/// What is wrong with a transcript file.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Read a transcript. Lines starting `//` or `#` are comments.
pub fn parse(text: &str) -> Result<Vec<Step>, ParseError> {
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            steps.push(Step::Ignored);
            continue;
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|e| ParseError {
            line,
            message: format!("not JSON: {e}"),
        })?;
        let Some(object) = value.as_object() else {
            return Err(ParseError {
                line,
                message: "every step is a JSON object with one key, 'core' or 'plugin'.".into(),
            });
        };
        match (object.get("core"), object.get("plugin")) {
            (Some(v), None) => steps.push(Step::Core(v.clone())),
            (None, Some(v)) => steps.push(Step::Plugin(v.clone())),
            _ => {
                return Err(ParseError {
                    line,
                    message: "a step has exactly one key: 'core' for a line the core sends, \
                              'plugin' for a line the plugin must send."
                        .into(),
                })
            }
        }
    }
    Ok(steps)
}

/// The steps that matter, with their line numbers, comments dropped.
pub fn steps(text: &str) -> Result<Vec<(usize, Step)>, ParseError> {
    Ok(parse(text)?
        .into_iter()
        .enumerate()
        .filter(|(_, s)| !matches!(s, Step::Ignored))
        .map(|(i, s)| (i + 1, s))
        .collect())
}

/// Does `actual` contain everything `expected` asks for?
///
/// Objects are matched key by key and may carry extra keys. Arrays must be the
/// same length and match element by element. `"*"` matches anything.
pub fn matches(expected: &Value, actual: &Value) -> bool {
    if expected == &Value::String("*".into()) {
        return true;
    }
    match (expected, actual) {
        (Value::Object(want), Value::Object(got)) => want
            .iter()
            .all(|(key, value)| got.get(key).is_some_and(|a| matches(value, a))),
        (Value::Array(want), Value::Array(got)) => {
            want.len() == got.len() && want.iter().zip(got).all(|(w, g)| matches(w, g))
        }
        _ => expected == actual,
    }
}

/// Say what did not match, for an error message a person can act on.
pub fn explain(expected: &Value, actual: &Value) -> String {
    format!(
        "expected a line containing {}\nbut got {}",
        serde_json::to_string(expected).unwrap_or_default(),
        serde_json::to_string(actual).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAMPLE: &str = r#"
# the handshake
{"plugin": {"method": "initialize", "params": {"plugin": "clock"}}}
{"core":   {"jsonrpc": "2.0", "id": 0, "result": {"canvas": {"width": 1280, "height": 720, "fps": 30}}}}
{"plugin": {"method": "initialized"}}
"#;

    #[test]
    fn a_transcript_parses_into_steps_in_order() {
        let got = steps(SAMPLE).unwrap();
        assert_eq!(got.len(), 3);
        assert!(matches!(got[0].1, Step::Plugin(_)));
        assert!(matches!(got[1].1, Step::Core(_)));
        assert!(matches!(got[2].1, Step::Plugin(_)));
    }

    #[test]
    fn a_step_with_two_keys_is_refused_with_its_line_number() {
        let err = parse("{\"core\": {}, \"plugin\": {}}\n").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("exactly one key"));
    }

    #[test]
    fn a_bad_line_names_its_number() {
        let err = parse("{\"core\": {}}\nnot json\n").unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn a_subset_matches_and_extra_keys_are_allowed() {
        let expected = json!({"method": "log", "params": {"level": "info"}});
        let actual = json!({"jsonrpc": "2.0", "method": "log",
                            "params": {"level": "info", "message": "hello"}});
        assert!(matches(&expected, &actual));
    }

    #[test]
    fn a_missing_key_does_not_match() {
        let expected = json!({"method": "log", "params": {"level": "warn"}});
        let actual = json!({"method": "log", "params": {"level": "info"}});
        assert!(!matches(&expected, &actual));
        assert!(explain(&expected, &actual).contains("but got"));
    }

    #[test]
    fn a_star_matches_anything() {
        assert!(matches(&json!({"id": "*"}), &json!({"id": 41})));
        assert!(matches(&json!({"id": "*"}), &json!({"id": {"deep": true}})));
        assert!(!matches(&json!({"id": "*"}), &json!({})));
    }

    #[test]
    fn arrays_match_element_by_element() {
        assert!(matches(&json!([1, "*"]), &json!([1, 2])));
        assert!(!matches(&json!([1]), &json!([1, 2])));
    }
}
