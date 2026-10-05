//! The `fx.*` method table: what each one is called, who may call it, and
//! the tool description a model reads. Written for a small model in
//! opencode or pi as much as for Claude: each says what to pass, with an
//! example, and which call comes next.

use super::*;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("fx.list", Scope::Read, "The imported transitions and effects, with what each is and whether it runs on the GPU here.", handler(list))
            .params(schema_of::<FxListRequest>)
            .result(schema_of::<FxList>)
            .tool(
                "list_fx",
                Tier::Search,
                "List the imported transitions and effects: light leaks, bokeh, glitches, film burns, \
                 stingers, luma wipes and shader transitions. Each has a `name`, a `kind` (stinger, \
                 overlay, matte, shader), a `blend`, `transition` true when `take` can use it and \
                 `effect` true when `fire_fx` can play it. To use one in a cut: take {\"scene\": \
                 \"wide\", \"transition\": \"light-leak\"}. To play an effect over what is on air: \
                 fire_fx {\"name\": \"bokeh\"}. Pass {\"role\": \"effect\"} or {\"role\": \
                 \"transition\"} to see only those.",
            ),
    );
    reg.register(
        MethodDef::new("fx.import", Scope::Operate, "Import a transition or effect from a file, a folder or a zip on the mixer's machine, measuring what it is and where it covers the picture.", handler(import))
            .params(schema_of::<FxImportRequest>)
            .result(schema_of::<FxImported>)
            .not_idempotent()
            .tool(
                "import_fx",
                Tier::Search,
                "Import a transition or effect pack: a clip, a picture, a .glsl shader, a folder or a \
                 .zip, by its path on the mixer's machine or its name in the media library. The mixer \
                 decides what each file is by looking at it (alpha means a stinger, light on black \
                 means a Screen overlay, a grey ramp means a luma wipe) and finds the frame where it \
                 covers the picture, which is where the cut goes. Example: import_fx {\"path\": \
                 \"C:/Users/me/Downloads/light-leaks.zip\"}. Override what it decided with `kind`, \
                 `blend` or `cut_at_ms`. A big pack answers with a task_id; read it with task_get.",
            ),
    );
    reg.register(
        MethodDef::new("fx.set", Scope::Operate, "Change an imported item: its blend, its cut point, its length, whether it is a transition or an effect.", handler(set))
            .params(schema_of::<FxSetRequest>)
            .result(schema_of::<FxEntry>)
            .tool(
                "set_fx",
                Tier::Search,
                "Change an imported transition or effect. `blend` is normal, screen, add or luma; \
                 `cut_at_ms` moves where the scenes swap under a clip (0 puts back the measured \
                 frame); `duration_ms` is a matte or shader's length; `transition` and `effect` turn \
                 its two uses on or off. Example: set_fx {\"name\": \"light-leak\", \"blend\": \
                 \"add\"}.",
            ),
    );
    reg.register(
        MethodDef::new("fx.remove", Scope::Operate, "Delete an imported item from the library. The starter set cannot be deleted.", handler(remove))
            .params(schema_of::<FxNameRequest>)
            .destructive()
            .tool("remove_fx", Tier::Search, "Delete an imported transition or effect from the mixer's library by its name. The starter set is refused; turn one off with set_fx instead."),
    );
    reg.register(
        MethodDef::new("fx.fire", Scope::Operate, "Play an effect over the programme once: drawn on top of whatever is on air until its clip ends.", handler(fire))
            .params(schema_of::<FxFireRequest>)
            .result(schema_of::<FxFired>)
            .not_idempotent()
            .tool(
                "fire_fx",
                Tier::Search,
                "Play an effect over the programme once, on top of whatever is on air, without \
                 changing the shot: a light leak sweep, bokeh, a glitch hit, a film burn. It goes by \
                 itself when its clip ends; the answer says how long that is. Example: fire_fx \
                 {\"name\": \"glitch\"}. Optional `opacity` 0 to 1 and `blend`. Only items with \
                 `effect` true in list_fx can be fired.",
            ),
    );
    reg.register(
        MethodDef::new("fx.preview", Scope::Read, "A moving preview of an item: twelve frames side by side in one JPEG, made once and kept.", handler(preview))
            .params(schema_of::<FxNameRequest>)
            .result(schema_of::<FxPreview>)
            .tool(
                "preview_fx",
                Tier::Search,
                "Get the preview strip of a transition or effect: a JPEG URL of twelve frames side by \
                 side over a blue scene changing to an orange one, so you can see what it looks like \
                 before using it on air.",
            ),
    );
    reg.register(
        MethodDef::new("fx.assign", Scope::Operate, "Choose the transition a take uses when it names none, for one scene or for every take.", handler(assign))
            .params(schema_of::<FxAssignRequest>)
            .result(schema_of::<FxAssignments>)
            .tool(
                "assign_transition",
                Tier::Search,
                "Choose the transition a take uses when it names none: for one scene, or for every take \
                 when `scene` is left out. `transition` is any name list_transitions shows, built in \
                 (fade, wipe) or imported (light-leak, glitch). Example: assign_transition {\"scene\": \
                 \"Interview\", \"transition\": \"light-leak\"}. Leave out `transition` to clear it. A \
                 take that names its own transition, or names cut, is not changed by this.",
            ),
    );
}
