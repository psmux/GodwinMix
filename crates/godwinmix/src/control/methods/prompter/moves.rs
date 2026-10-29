//! `prompter.load`, `play`, `pause`, `speed`, `seek` and `set`.
//!
//! Each answers the whole state, the same document `event/prompter.changed`
//! carries to every screen.

use super::super::{body, handler};
use crate::control::call::Call;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::prompter::*;
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    register_transport(reg);
    register_place(reg);
}

fn register_transport(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "prompter.load",
            Scope::Operate,
            "Put a script on the prompter, at the top and paused unless play is true.",
            handler(|call: Call, params: Value| async move {
                let req: LoadRequest = call.params(&params)?;
                body(call.app.prompter.load(req)?)
            }),
        )
        .params(schema_of::<LoadRequest>)
        .result(schema_of::<PrompterState>)
        .tool(
            "load_prompter_script",
            Tier::Search,
            "Put a teleprompter script on every prompter screen: {id}. It starts at the top, \
             paused; send play: true to start it scrolling at once.",
        ),
    );

    reg.register(
        MethodDef::new(
            "prompter.play",
            Scope::Operate,
            "Start the prompter scrolling, at speed when given. From the end it starts again at the top.",
            handler(|call: Call, params: Value| async move {
                let req: PlayRequest = call.params(&params)?;
                body(call.app.prompter.play(req.speed)?)
            }),
        )
        .params(schema_of::<PlayRequest>)
        .result(schema_of::<PrompterState>)
        .tool(
            "prompter_play",
            Tier::Search,
            "Start the teleprompter scrolling. Optional speed in rows a second (0.4 is an easy \
             reading pace, 1 is quick). Needs a script loaded first.",
        ),
    );

    reg.register(
        MethodDef::new(
            "prompter.pause",
            Scope::Operate,
            "Stop the prompter where it is.",
            handler(|call: Call, _| async move { body(call.app.prompter.pause()) }),
        )
        .result(schema_of::<PrompterState>)
        .tool(
            "prompter_pause",
            Tier::Search,
            "Stop the teleprompter where it is, on every screen at once. prompter_play carries on \
             from the same place at the same speed.",
        ),
    );

    reg.register(
        MethodDef::new(
            "prompter.speed",
            Scope::Operate,
            "Set the speed in rows a second (speed), or change it (by). Negative scrolls back. \
             The screens carry on from where they are, so it can be sent as often as wanted.",
            handler(|call: Call, params: Value| async move {
                let req: SpeedRequest = call.params(&params)?;
                body(call.app.prompter.speed(req)?)
            }),
        )
        .params(schema_of::<SpeedRequest>)
        .result(schema_of::<PrompterState>)
        .not_idempotent()
        .tool(
            "prompter_speed",
            Tier::Search,
            "Change how fast the teleprompter scrolls: {speed: 0.6} sets it in rows a second, \
             {by: 0.1} or {by: -0.1} nudges it. -20 to 20; negative scrolls back. It does not \
             start or stop it.",
        ),
    );
}

fn register_place(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "prompter.seek",
            Scope::Operate,
            "Move the prompter: to a row, to a fraction of the way through, to a marker by name \
             or number, a number of markers on or back (step), or a number of rows (by).",
            handler(|call: Call, params: Value| async move {
                let req: SeekRequest = call.params(&params)?;
                body(call.app.prompter.seek(req)?)
            }),
        )
        .params(schema_of::<SeekRequest>)
        .result(schema_of::<PrompterState>)
        .not_idempotent()
        .tool(
            "prompter_seek",
            Tier::Search,
            "Jump the teleprompter. Exactly one of: {marker: \"Weather\"} (or \"2\" for the second \
             marker), {step: 1} next marker, {step: -1} back, {fraction: 0.5} half way, {row: 0} \
             the top, {by: -3} three rows back. It keeps playing or stays paused.",
        ),
    );

    reg.register(
        MethodDef::new(
            "prompter.set",
            Scope::Operate,
            "Change how the text looks on every screen: line_chars (the text size, as characters \
             a row), margin, mirror and the cue line. Only what is named changes.",
            handler(|call: Call, params: Value| async move {
                let req: LookRequest = call.params(&params)?;
                body(call.app.prompter.set_look(req)?)
            }),
        )
        .params(schema_of::<LookRequest>)
        .result(schema_of::<PrompterState>),
    );
}
