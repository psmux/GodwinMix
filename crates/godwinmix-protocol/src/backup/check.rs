//! The checks an `on_loss` rule has to pass before anything acts on it.
//!
//! Written once and used twice: by the config loader, so a file with a
//! backup that does not exist refuses to start rather than failing in the
//! middle of a show, and by `source.on_loss.set`, which adds the transition
//! plugins that happen to be running.

use super::{OnLoss, MAX_TRANSITION_MS, MAX_WAIT_SECS};
use serde_json::{json, Map, Value};

/// Why a rule was refused: a sentence that says what to do, and the facts a
/// client can act on.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub message: String,
    pub data: Map<String, Value>,
}

impl Problem {
    fn new(message: String, data: Value) -> Self {
        let data = match data {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        Problem { message, data }
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Problem {}

/// Whether a take could run this transition without a plugin.
pub fn known_transition(name: &str) -> bool {
    let name = name.trim().to_lowercase();
    name == "dissolve" || crate::requests::TRANSITIONS.contains(&name.as_str())
}

/// Check every rule against the source ids there are, and against each other.
///
/// `rules` is each source that has one, by id; `ids` is every source the
/// mixer has. A backup that is not there, a source that backs itself up and
/// a loop of backups are refused, each saying how to put it right.
pub fn check(rules: &[(&str, &OnLoss)], ids: &[&str]) -> Result<(), Problem> {
    for (id, rule) in rules {
        one(id, rule, ids)?;
    }
    for (id, _) in rules.iter().rev() {
        if let Some(cycle) = loop_from(id, rules) {
            let shown = cycle.join(" -> ");
            return Err(Problem::new(
                format!(
                    "the backups go round in a loop: {shown}. A source cannot stand in for \
                     one that stands in for it. Clear one of them with source.on_loss.clear, \
                     or pick a backup outside the loop."
                ),
                json!({ "source": id, "cycle": cycle }),
            ));
        }
    }
    Ok(())
}

/// One rule on its own: its backup exists, and its numbers are in range.
fn one(id: &str, rule: &OnLoss, ids: &[&str]) -> Result<(), Problem> {
    let backup = rule.backup.trim();
    if backup.is_empty() {
        return Err(Problem::new(
            format!("{id} has on_loss with no backup. Name the source to show instead: backup = \"cam-wide\"."),
            json!({ "source": id, "sources": ids }),
        ));
    }
    if backup == id {
        return Err(Problem::new(
            format!("{id} cannot be its own backup. Pick another source to show when it goes down."),
            json!({ "source": id, "sources": ids }),
        ));
    }
    if !ids.contains(&backup) {
        let others: Vec<&str> = ids.iter().copied().filter(|s| *s != id).collect();
        return Err(Problem::new(
            format!(
                "{id} falls back to {backup}, and there is no source called {backup}. Pick one \
                 of: {}. Add it first if it is new.",
                if others.is_empty() { "(no other sources yet)".to_string() } else { others.join(", ") }
            ),
            json!({ "source": id, "backup": backup, "sources": others }),
        ));
    }
    for (name, secs) in [("after_secs", rule.after_secs), ("return_after_secs", rule.return_after_secs)] {
        if !secs.is_finite() || !(0.0..=MAX_WAIT_SECS).contains(&secs) {
            return Err(Problem::new(
                format!("{id}'s on_loss {name} is {secs}; it has to be between 0 and {MAX_WAIT_SECS} seconds."),
                json!({ "source": id, "field": name, "min": 0, "max": MAX_WAIT_SECS }),
            ));
        }
    }
    if rule.duration_ms.is_some_and(|ms| ms > MAX_TRANSITION_MS) {
        return Err(Problem::new(
            format!("{id}'s on_loss duration_ms is over {MAX_TRANSITION_MS}; a transition runs ten seconds at most."),
            json!({ "source": id, "field": "duration_ms", "max": MAX_TRANSITION_MS }),
        ));
    }
    if rule.transition_name() == "stinger" {
        return Err(Problem::new(
            format!("{id}'s on_loss cannot use a stinger, which needs a clip of its own. Use fade, wipe, dip or cut."),
            json!({ "source": id, "field": "transition" }),
        ));
    }
    Ok(())
}

/// The loop starting at `start`, if following its backups comes back to it.
fn loop_from(start: &str, rules: &[(&str, &OnLoss)]) -> Option<Vec<String>> {
    let rule_of = |id: &str| rules.iter().find(|(s, _)| *s == id).map(|(_, r)| r.backup.trim().to_string());
    let mut path = vec![start.to_string()];
    let mut at = rule_of(start)?;
    while path.len() <= rules.len() {
        path.push(at.clone());
        if at == start {
            return Some(path);
        }
        if path[..path.len() - 1].contains(&at) {
            // A loop that does not pass through `start`; it is reported
            // when the walk starts from one of its own members.
            return None;
        }
        at = rule_of(&at)?;
    }
    None
}
