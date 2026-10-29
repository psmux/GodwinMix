//! `prompter.*`: the teleprompter's scripts and its place in them.
//!
//! The work is in `crate::prompter`; these are the rows of the method table.
//! The moves (load, play, pause, speed, seek, the look) are in `moves.rs`.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::prompter::*;
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

mod moves;

pub fn register(reg: &mut Registry<Call>) {
    register_reads(reg);
    register_scripts(reg);
    moves::register(reg);
}

fn register_reads(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "prompter.get",
            Scope::Read,
            "The prompter now: the script on it, its rows and markers, the position at at_ms on \
             the mixer's clock, the speed in rows a second, whether it plays, and the look.",
            handler(|call: Call, _| async move { body(call.app.prompter.state()) }),
        )
        .result(schema_of::<PrompterState>)
        .tool(
            "prompter_state",
            Tier::Search,
            "Where the teleprompter is: which script is loaded, position (in rows) at at_ms, speed \
             in rows a second, playing or not, the markers with the row each starts on, and the \
             look (line_chars, margin, mirror, cue). The position now is position + speed * \
             (now_ms - at_ms) / 1000 while it plays.",
        ),
    );

    reg.register(
        MethodDef::new(
            "prompter.rows",
            Scope::Read,
            "The script on the prompter broken into rows at line_chars, the unit every position \
             is counted in. A screen asks again when updated_ms or line_chars changes.",
            handler(|call: Call, _| async move { body(call.app.prompter.rows()) }),
        )
        .result(schema_of::<RowsAnswer>),
    );

    reg.register(
        MethodDef::new(
            "prompter.script.list",
            Scope::Read,
            "Every prompter script kept on the station, with its title, size and markers, and \
             which one is on the prompter. Without the text.",
            handler(|call: Call, _| async move { body(call.app.prompter.list()) }),
        )
        .result(schema_of::<ScriptList>)
        .tool(
            "list_prompter_scripts",
            Tier::Search,
            "The teleprompter scripts this show keeps: id, title, line count, marker names. `on` \
             is the one loaded now. Read one with prompter.script.get.",
        ),
    );

    reg.register(
        MethodDef::new(
            "prompter.script.get",
            Scope::Read,
            "One prompter script with its text.",
            handler(|call: Call, params: Value| async move {
                let req: ScriptIdRequest = call.params(&params)?;
                body(call.app.prompter.script(&req.id)?)
            }),
        )
        .params(schema_of::<ScriptIdRequest>)
        .result(schema_of::<Script>),
    );
}

fn register_scripts(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "prompter.script.save",
            Scope::Operate,
            "Make a prompter script or replace the one with the same id. A line starting with # \
             is a marker; *word* is italic and **word** bold. Saving the script on the prompter \
             changes every screen at once and keeps the place.",
            handler(|call: Call, params: Value| async move {
                let req: ScriptSaveRequest = call.params(&params)?;
                body(call.app.prompter.save(req)?)
            }),
        )
        .params(schema_of::<ScriptSaveRequest>)
        .result(schema_of::<Script>)
        .tool(
            "save_prompter_script",
            Tier::Search,
            "Write a teleprompter script: {title: \"Evening news\", text: \"# Opening\\nGood \
             evening...\"}. The id is made from the title unless you give one, and saving with an \
             existing id replaces it. Start a line with # to make a marker the presenter can jump \
             to. Keep lines short; the screens break long ones. Then load_prompter_script to put \
             it on.",
        ),
    );

    reg.register(
        MethodDef::new(
            "prompter.script.remove",
            Scope::Operate,
            "Forget a prompter script. When it is on the prompter the screens go empty.",
            handler(|call: Call, params: Value| async move {
                let req: ScriptIdRequest = call.params(&params)?;
                body(call.app.prompter.remove(&req.id)?)
            }),
        )
        .params(schema_of::<ScriptIdRequest>)
        .result(schema_of::<ScriptChanged>),
    );
}
