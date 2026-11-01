//! The `lut.*` method table: what each one is called, who may call it, and
//! the tool description a model reads. Each says what to pass, with an
//! example, and which call comes next.

use super::*;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("lut.list", Scope::Read, "Every LUT the mixer has (camera conversions, built in looks, imported ones) and the saved presets.", handler(list))
            .params(schema_of::<LutListRequest>)
            .result(schema_of::<LutList>)
            .tool(
                "list_luts",
                Tier::Search,
                "List the looks this mixer has. `luts` holds camera conversions from log to Rec.709 \
                 (group camera: slog3-to-709, vlog-to-709, clog3-to-709, logc3-to-709, flog-to-709, \
                 nlog-to-709, hlg-to-709), built in grades (group look: warm, cool, teal-orange, \
                 faded-film, bleach-bypass, high-contrast, vivid, black-and-white, sepia, \
                 day-for-night) and imported ones. `presets` are saved looks. To put one on a \
                 camera, add a filter: add_filter {\"type\": \"lut/filter\", \"params\": {\"lut\": \
                 \"teal-orange\", \"amount\": 0.7}}, or {\"preset\": \"church-warm\"}.",
            ),
    );
    reg.register(
        MethodDef::new("lut.import", Scope::Operate, "Import a .cube, .3dl or PNG LUT from the mixer's machine or the media library into the LUT library.", handler(import))
            .params(schema_of::<LutImportRequest>)
            .result(schema_of::<LutImported>)
            .not_idempotent()
            .tool(
                "import_lut",
                Tier::Search,
                "Import a LUT file: a .cube (Resolve, Adobe, camera makers), a .3dl or a PNG (OBS's \
                 512 square or a Hald CLUT), by its path on the mixer's machine or its name in the \
                 media library. It is kept as a .cube under a slug. Example: import_lut {\"path\": \
                 \"C:/Users/me/Downloads/Kodak 2383.cube\", \"name\": \"kodak\"}. Then use it as \
                 {\"lut\": \"kodak\"} on a lut/filter.",
            ),
    );
    reg.register(
        MethodDef::new("lut.remove", Scope::Operate, "Delete an imported LUT. The built in ones cannot be deleted.", handler(remove))
            .params(schema_of::<LutNameRequest>)
            .destructive()
            .tool(
                "remove_lut",
                Tier::Search,
                "Delete an imported LUT from the mixer's library by its name, as list_luts shows it. \
                 The built in camera conversions and looks are refused. A lut/filter that names the \
                 deleted LUT passes its picture through unchanged until one of that name is imported \
                 again. Example: remove_lut {\"name\": \"kodak\"}.",
            ),
    );
    reg.register(
        MethodDef::new("lut.preset.save", Scope::Operate, "Save a look under a name: a LUT, how much of it, and a colour correction before it.", handler(preset_save))
            .params(schema_of::<LutPreset>)
            .result(schema_of::<LutPreset>)
            .tool(
                "save_lut_preset",
                Tier::Search,
                "Save a preset, replacing one of the same name. Example: save_lut_preset {\"name\": \
                 \"church-warm\", \"title\": \"Church warm\", \"lut\": \"warm\", \"amount\": 0.6, \
                 \"saturation\": 0.1}. Use it on a lut/filter as {\"preset\": \"church-warm\"}.",
            ),
    );
    reg.register(
        MethodDef::new("lut.preset.remove", Scope::Operate, "Delete a saved preset.", handler(preset_remove))
            .params(schema_of::<LutNameRequest>)
            .destructive()
            .tool(
                "remove_lut_preset",
                Tier::Search,
                "Delete a saved look preset by its name, as list_luts shows it under `presets`. A \
                 lut/filter that names the preset keeps the look it already has on air, and refuses \
                 the next change that names it. Example: remove_lut_preset {\"name\": \"church-warm\"}.",
            ),
    );
    reg.register(
        MethodDef::new("lut.preview", Scope::Read, "Small JPEG pictures of looks on a source, or on a test card, for a picker.", handler(preview_looks))
            .params(schema_of::<LutPreviewRequest>)
            .result(schema_of::<LutPreviews>)
            .tool(
                "preview_luts",
                Tier::Search,
                "Show what looks do to a camera: small JPEG data URLs of the source's picture through \
                 each LUT or preset, and the original. Example: preview_luts {\"source\": \"cam-wide\", \
                 \"luts\": [\"warm\", \"teal-orange\"]}. Leave out `luts` for every one.",
            ),
    );
}
