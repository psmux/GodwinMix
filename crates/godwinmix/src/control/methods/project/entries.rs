//! Sources and outputs from a project file, set against the ones running here.
//!
//! One rule for both, because both are a list of configs keyed by a slug.
//! Replace: the file's list becomes this mixer's; an entry that is the same
//! here is kept rather than restarted, and one the file carries without its
//! key keeps this mixer's copy, which still has it. Merge: everything in the
//! file is added, renamed (`cam-wide-2`) where the id is taken.

use super::redact;
use super::{Change, Report};
use godwinmix_protocol::error::RpcError;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// What to do with one kind of entry.
pub struct Moves<T> {
    pub remove: Vec<String>,
    pub add: Vec<T>,
    /// Old id to new, for a merge that renamed.
    pub renamed: BTreeMap<String, String>,
}

/// Work out the moves for `kind` ("source" or "output").
pub fn plan<T: Serialize + DeserializeOwned + Clone>(
    kind: &str,
    file: &[Value],
    here: &[T],
    id_of: fn(&T) -> &str,
    set_id: fn(&mut T, String),
    replace: bool,
    report: &mut Report,
) -> Result<Moves<T>, RpcError> {
    let Report { changes, waiting, .. } = report;
    let mut moves = Moves { remove: Vec::new(), add: Vec::new(), renamed: BTreeMap::new() };
    let mut taken: Vec<String> = here.iter().map(|t| id_of(t).to_string()).collect();
    let mut named_in_file = Vec::new();
    for (n, value) in file.iter().enumerate() {
        let lost = redact::removed_from(value);
        let mut plain = value.clone();
        if let Some(o) = plain.as_object_mut() {
            o.remove(redact::REMOVED);
        }
        let mut entry: T = serde_json::from_value(plain).map_err(|e| damaged(kind, n, value, e))?;
        let id = id_of(&entry).to_string();
        named_in_file.push(id.clone());
        let existing = here.iter().find(|t| id_of(t) == id);
        if replace {
            match existing {
                Some(old) if same(old, &entry) => changes.push(Change::new(kind, &id, "keep")),
                Some(_) if !lost.is_empty() => changes.push(
                    Change::new(kind, &id, "keep").note("the file left out its key, so this mixer's copy stays"),
                ),
                Some(_) => {
                    moves.remove.push(id.clone());
                    moves.add.push(entry);
                    changes.push(Change::new(kind, &id, "replace"));
                }
                None if !lost.is_empty() => wait(kind, &id, &lost, changes, waiting),
                None => {
                    moves.add.push(entry);
                    changes.push(Change::new(kind, &id, "add"));
                }
            }
            continue;
        }
        let mut change = Change::new(kind, &id, "add");
        if taken.contains(&id) {
            let to = free(&id, &taken);
            moves.renamed.insert(id.clone(), to.clone());
            set_id(&mut entry, to.clone());
            change = Change::new(kind, &id, "rename").to(&to);
        }
        taken.push(id_of(&entry).to_string());
        if !lost.is_empty() {
            wait(kind, id_of(&entry), &lost, changes, waiting);
            continue;
        }
        moves.add.push(entry);
        changes.push(change);
    }
    if replace {
        for old in here.iter().map(id_of).filter(|id| !named_in_file.iter().any(|n| n == id)) {
            moves.remove.push(old.to_string());
            changes.push(Change::new(kind, old, "remove"));
        }
    }
    Ok(moves)
}

fn wait(kind: &str, id: &str, lost: &[String], changes: &mut Vec<Change>, waiting: &mut Vec<String>) {
    let what = lost.join(", ");
    changes.push(Change::new(kind, id, "wait").note(&format!("the file left out {what}")));
    waiting.push(format!(
        "{kind} {id} is not started: the file was saved without {what}. Add it again from the page, or save the project with keys included."
    ));
}

fn same<T: Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

/// `base`, or `base-2`, `base-3`, whichever is not taken.
pub fn free(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base}-{n}")).find(|id| !taken.contains(id)).expect("an unused number")
}

fn damaged(kind: &str, n: usize, value: &Value, e: serde_json::Error) -> RpcError {
    let id = value.get("id").and_then(Value::as_str).unwrap_or("with no id");
    RpcError::invalid_params(format!(
        "{kind} {} in the file ({id}) is damaged: {e}. Nothing was changed. Export the project again from the mixer it came from.",
        n + 1
    ))
    .with("field", "file")
    .with("reason", "damaged")
    .with(kind, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    struct Thing {
        id: String,
        #[serde(default)]
        uri: String,
    }

    fn id_of(t: &Thing) -> &str {
        &t.id
    }
    fn set_id(t: &mut Thing, id: String) {
        t.id = id;
    }

    #[test]
    fn a_merge_renames_on_a_clash_and_a_replace_takes_the_rest_away() {
        let here = vec![Thing { id: "cam".into(), uri: "a".into() }, Thing { id: "old".into(), uri: "b".into() }];
        let file = vec![json!({"id": "cam", "uri": "c"}), json!({"id": "yt", "uri": "rtmp://x/live", "$removed": ["its stream key"]})];
        let mut report = Report::default();
        let merged = plan("source", &file, &here, id_of, set_id, false, &mut report).unwrap();
        assert_eq!(merged.renamed.get("cam").map(String::as_str), Some("cam-2"));
        assert_eq!(merged.add.len(), 1, "the one without its key waits");
        assert_eq!(report.waiting.len(), 1);
        assert!(merged.remove.is_empty());

        let mut report = Report::default();
        let replaced = plan("source", &file, &here, id_of, set_id, true, &mut report).unwrap();
        assert_eq!(replaced.remove, vec!["cam".to_string(), "old".to_string()]);
        assert_eq!(replaced.add.len(), 1);
        assert!(report.changes.iter().any(|c| c.id == "old" && c.action == "remove"));
    }

    #[test]
    fn a_damaged_entry_names_itself() {
        let file = vec![json!({"uri": 7})];
        let e = plan::<Thing>("source", &file, &[], id_of, set_id, true, &mut Report::default()).err().unwrap();
        assert!(e.message.contains("source 1 in the file"), "{}", e.message);
    }
}
