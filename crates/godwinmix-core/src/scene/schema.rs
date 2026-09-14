//! The published JSON Schema for the scene document.
//!
//! `schemas/scene.schema.json` is generated from the Rust types and committed,
//! so a client in any language validates against the same thing the core reads,
//! and a test fails the build when the two drift. Nobody has to remember to
//! update it, which is the only way a hand written schema ever stays true.

use serde_json::Value;

/// The path of the committed schema, relative to the repository root.
pub const PATH: &str = "schemas/scene.schema.json";

/// The schema as this build's types define it, pretty printed with a trailing
/// newline so the file is a well formed text file.
pub fn generate() -> String {
    let mut root = serde_json::to_value(schemars::schema_for!(crate::scene::document::Collection))
        .expect("a schema is always JSON");
    // The generator writes the type name; the document deserves a title and a
    // pointer back to what it is for.
    if let Some(object) = root.as_object_mut() {
        object.insert("title".into(), Value::from("GodwinMix scene collection"));
        object.insert(
            "description".into(),
            Value::from(
                "A GodwinMix scene collection: canvas, parameters, scenes of items, transitions and assets. See docs/reference/scene-document.md.",
            ),
        );
    }
    let mut text = serde_json::to_string_pretty(&root).expect("a schema always serialises");
    text.push('\n');
    text
}

/// The schema for the flat record store, which is what `scene.*` carries on the
/// wire. Printed on demand rather than committed: the document is the published
/// contract, and this is its projection.
pub fn generate_flat() -> String {
    let mut text =
        serde_json::to_string_pretty(&schemars::schema_for!(crate::scene::flat::FlatDocument))
            .expect("a schema always serialises");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed file, read from the source tree rather than the working
    /// directory, so the test passes wherever cargo is run from.
    fn committed() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(PATH)
    }

    #[test]
    fn the_committed_schema_matches_the_types() {
        let on_disk = std::fs::read_to_string(committed()).unwrap_or_default();
        // Compare the parsed JSON, not the bytes, so a checkout that rewrote
        // the line endings does not fail the build.
        let a: Value = serde_json::from_str(&on_disk).unwrap_or(Value::Null);
        let b: Value = serde_json::from_str(&generate()).expect("the generated schema is JSON");
        assert_eq!(
            a, b,
            "{PATH} is out of date. Regenerate it with `gmx scene schema > {PATH}` and commit the result."
        );
    }

    #[test]
    fn the_schema_says_draft_2020_12_and_describes_an_item() {
        let schema: Value = serde_json::from_str(&generate()).unwrap();
        assert_eq!(schema["$schema"], "https://json-schema.org/draft/2020-12/schema");
        let defs = &schema["$defs"];
        assert!(defs["Item"].is_object(), "no Item definition: {defs}");
        assert!(defs["Fit"].is_object(), "no Fit definition");
        assert_eq!(defs["Id"]["type"], "string");
    }

    #[test]
    fn the_flat_schema_generates_too() {
        let schema: Value = serde_json::from_str(&generate_flat()).unwrap();
        assert!(schema["$defs"]["Record"].is_object(), "no Record definition");
    }
}
