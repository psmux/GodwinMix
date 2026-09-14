//! Schema migrations for the scene document.
//!
//! The table exists before it is needed, which is the point. OBS carries a
//! `versioned_id` on every source because settings shapes changed after the
//! files were in the field, and the collections that predate it are migrated by
//! guesswork. Here every version has one step to the next, the steps run in
//! order, and version 0 to 1 is a no op that proves the mechanism runs.

use anyhow::{bail, Result};
use serde_json::Value;

use super::document::SCHEMA_VERSION;

/// One step: the version it reads, and what it changes in place. After it runs
/// the caller stamps the document with `from + 1`.
type Step = (u32, fn(&mut Value) -> Result<()>);

/// Every step, in order. Add to the end, never edit a step that has shipped.
const STEPS: &[Step] = &[(0, v0_to_v1)];

/// Version 0 is any document written before the version was stamped. Nothing
/// about the shape changed, so this step only exists to carry the version
/// forward and to keep the machinery honest.
fn v0_to_v1(_doc: &mut Value) -> Result<()> {
    Ok(())
}

/// Bring a document up to `SCHEMA_VERSION`, in place.
pub fn migrate(doc: &mut Value) -> Result<()> {
    let Some(object) = doc.as_object_mut() else {
        bail!(
            "a scene document is a JSON object, and this file holds a {}",
            kind_of(doc)
        );
    };
    let mut version = object
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or(0);
    if version > SCHEMA_VERSION {
        bail!(
            "this document is at schema version {version} and this build of GodwinMix reads up to {SCHEMA_VERSION}. Update GodwinMix, or export the document from the newer one at version {SCHEMA_VERSION}."
        );
    }
    while version < SCHEMA_VERSION {
        let Some((_, step)) = STEPS.iter().find(|(from, _)| *from == version) else {
            bail!(
                "no migration from schema version {version} to {}. This document was written by a build that is not on the release line.",
                version + 1
            );
        };
        step(doc)?;
        version += 1;
        doc.as_object_mut()
            .expect("still an object")
            .insert("schemaVersion".into(), Value::from(version));
    }
    Ok(())
}

/// The JSON kind of a value, for an error message a person can act on.
fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_document_with_no_version_is_carried_to_the_current_one() {
        let mut doc = json!({ "name": "old" });
        migrate(&mut doc).unwrap();
        assert_eq!(doc["schemaVersion"], json!(SCHEMA_VERSION));
    }

    #[test]
    fn a_document_already_current_is_left_alone() {
        let mut doc = json!({ "schemaVersion": SCHEMA_VERSION, "name": "now" });
        let before = doc.clone();
        migrate(&mut doc).unwrap();
        assert_eq!(doc, before);
    }

    #[test]
    fn a_document_from_the_future_says_which_way_to_go() {
        let mut doc = json!({ "schemaVersion": SCHEMA_VERSION + 5 });
        let err = migrate(&mut doc).unwrap_err().to_string();
        assert!(err.contains("Update GodwinMix"), "{err}");
    }

    #[test]
    fn every_version_below_the_current_one_has_a_step() {
        for v in 0..SCHEMA_VERSION {
            assert!(
                STEPS.iter().any(|(from, _)| *from == v),
                "no migration step from version {v}"
            );
        }
    }

    #[test]
    fn a_file_that_is_not_an_object_says_what_it_found() {
        let mut doc = json!([1, 2, 3]);
        let err = migrate(&mut doc).unwrap_err().to_string();
        assert!(err.contains("array"), "{err}");
    }
}
