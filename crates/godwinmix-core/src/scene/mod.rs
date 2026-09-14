//! Scenes: the document, its two projections, the built in layouts, the
//! validator and the OBS importer.
//!
//! What is here is everything about a scene that is pure data. The live part
//! (the `scene.*` commands, the compositor's slot pool, undo) is the scene
//! server and belongs beside the mixer; it reads and writes these types.
//!
//! 11 section 2 is the specification these types follow, key for key.

pub mod document;
pub mod expr;
pub mod flat;
pub mod geometry;
pub mod id;
pub mod layout;
pub mod migrate;
pub mod obs_import;
pub mod order;
pub mod presets;
pub mod schema;
pub mod validate;

pub use document::{
    Align, Asset, Audio, Blend, Canvas, Collection, Content, Crop, Filter, Fit, Frame, Item,
    Override, Scene, Transform, Transition, Vec2, SCHEMA_VERSION,
};
pub use flat::{FlatContent, FlatDocument, ItemProps, Props, Record};
pub use id::Id;

#[cfg(test)]
mod doc_tests {
    //! The documentation for all of this is in `docs/`, and a document that
    //! quietly stops being true is worse than no document. These check the
    //! lists most likely to drift: the vocabularies, the layout names, the
    //! preset names, and the OBS types the importer claims to know.

    use std::path::PathBuf;

    fn doc(path: &str) -> String {
        let full = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(path);
        std::fs::read_to_string(&full)
            .unwrap_or_else(|e| panic!("{}: {e}. The docs are part of the feature.", full.display()))
    }

    #[test]
    fn the_reference_names_every_fit_and_align_keyword() {
        let text = doc("docs/reference/scene-document.md");
        for fit in ["none", "stretch", "contain", "cover", "fit-width", "fit-height", "max"] {
            assert!(text.contains(&format!("`{fit}`")), "the fit {fit:?} is not documented");
        }
        for align in [
            "top-left",
            "top-center",
            "top-right",
            "center-left",
            "center",
            "center-right",
            "bottom-left",
            "bottom-center",
            "bottom-right",
        ] {
            assert!(text.contains(align), "the alignment {align:?} is not documented");
        }
        assert!(text.contains(super::schema::PATH), "the reference does not point at the schema");
    }

    #[test]
    fn the_reference_names_every_validation_code() {
        let text = doc("docs/reference/scene-document.md");
        for code in [
            "scene.ref",
            "scene.duplicate_id",
            "scene.off_canvas",
            "scene.off_canvas_partly",
            "scene.hidden",
            "scene.action_safe",
            "scene.title_safe",
        ] {
            assert!(text.contains(code), "the finding {code:?} is not documented");
        }
    }

    #[test]
    fn the_preset_guide_names_every_layout_slot_panel_and_preset() {
        let text = doc("docs/how-to/make-a-preset.md");
        for name in super::presets::NAMES {
            assert!(text.contains(&format!("`{name}`")), "the preset {name:?} is not in the guide");
        }
        for slot in super::presets::SLOTS {
            assert!(text.contains(&format!("`{slot}`")), "the slot {slot:?} is not in the guide");
        }
        for panel in super::presets::PANELS {
            assert!(text.contains(&format!("`{panel}`")), "the panel {panel:?} is not in the guide");
        }
    }

    #[test]
    fn the_import_guide_names_every_obs_type_the_importer_knows() {
        let text = doc("docs/how-to/import-from-obs.md");
        for obs in [
            "ffmpeg_source",
            "vlc_source",
            "image_source",
            "browser_source",
            "v4l2_input",
            "av_capture_input",
            "dshow_input",
            "monitor_capture",
            "window_capture",
            "xshm_input",
            "text_gdiplus",
            "text_ft2_source",
            "color_source",
        ] {
            assert!(text.contains(obs), "the OBS type {obs:?} is not in the import guide");
        }
        // Where the file is on each platform is the first thing anybody needs.
        for place in ["%APPDATA%", "Library/Application Support", ".config/obs-studio"] {
            assert!(text.contains(place), "the guide does not say where OBS keeps its files on {place}");
        }
    }

    #[test]
    fn the_import_guide_says_what_the_report_means() {
        let text = doc("docs/how-to/import-from-obs.md");
        for phrase in ["imported as", "needs the", "skipped, because", "--report json", "--source-size"] {
            assert!(text.contains(phrase), "{phrase:?} is not explained in the import guide");
        }
    }
}
