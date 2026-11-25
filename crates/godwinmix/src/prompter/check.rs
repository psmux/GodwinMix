//! What the prompter refuses, each time with what to send instead.

use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::prompter::{Look, LookRequest, MAX_MARGIN, MAX_SCRIPT_BYTES, MAX_SPEED, LINE_CHARS};

fn field(name: &str, message: String) -> RpcError {
    RpcError::invalid_params(message).with("field", name.to_string())
}

/// A script id: a slug of lower case letters, digits and dashes.
pub fn id(id: &str) -> Result<(), RpcError> {
    if godwinmix_protocol::plugin::manifest::is_slug(id) {
        return Ok(());
    }
    let suggest = crate::channels::keys::slug(id);
    Err(field(
        "id",
        format!("'{id}' is not a script id: an id is lower case letters, digits and single dashes, starting with a letter. '{suggest}' would do."),
    )
    .with("suggest", suggest))
}

pub fn title_and_text(title: &str, text: &str) -> Result<(), RpcError> {
    if title.trim().is_empty() {
        return Err(field("title", "a script needs a title, which is what the desk and the screens call it. Send one with title.".into()));
    }
    if text.len() > MAX_SCRIPT_BYTES {
        return Err(field(
            "text",
            format!("this script is {} bytes and the most one may be is {MAX_SCRIPT_BYTES}. Split it into two scripts.", text.len()),
        )
        .with("bytes", text.len())
        .with("max_bytes", MAX_SCRIPT_BYTES));
    }
    Ok(())
}

pub fn speed(speed: f64) -> Result<f64, RpcError> {
    if speed.is_finite() && speed.abs() <= MAX_SPEED {
        return Ok(speed);
    }
    Err(field(
        "speed",
        format!("a speed of {speed} rows a second is refused: send between -{MAX_SPEED} and {MAX_SPEED}. Negative scrolls back, 0.4 reads at an easy pace."),
    )
    .with("min", -MAX_SPEED)
    .with("max", MAX_SPEED))
}

/// The look with the request applied, or the first value out of range.
pub fn look(mut look: Look, req: &LookRequest) -> Result<Look, RpcError> {
    if let Some(n) = req.line_chars {
        if !LINE_CHARS.contains(&n) {
            return Err(field("line_chars", format!("{n} characters a row is refused: send {} to {}. Fewer is bigger text.", LINE_CHARS.start(), LINE_CHARS.end())));
        }
        look.line_chars = n;
    }
    if let Some(m) = req.margin {
        if !(0.0..=MAX_MARGIN).contains(&m) {
            return Err(field("margin", format!("a margin of {m}% is refused: send 0 to {MAX_MARGIN}, the percentage of the width left on each side.")));
        }
        look.margin = m;
    }
    if let Some(c) = req.cue {
        if !(0.1..=0.9).contains(&c) {
            return Err(field("cue", format!("a cue line at {c} is refused: send 0.1 to 0.9, its place as a fraction of the height from the top.")));
        }
        look.cue = c;
    }
    if let Some(m) = req.mirror {
        look.mirror = m;
    }
    Ok(look)
}

/// No script by that id, naming the ones there are.
pub fn no_script(id: &str, ids: Vec<String>) -> RpcError {
    let next = if ids.is_empty() {
        "There are no scripts yet: save one with prompter.script.save.".to_string()
    } else {
        format!("The scripts are {}.", ids.join(", "))
    };
    RpcError::new(ErrorCode::NotFound, format!("there is no prompter script '{id}'. {next}"))
        .with("id", id.to_string())
        .with("scripts", ids)
}

/// A move asked of a prompter with nothing on it.
pub fn nothing_loaded(method: &str, ids: Vec<String>) -> RpcError {
    RpcError::new(
        ErrorCode::NotInState,
        format!("{method} needs a script on the prompter and there is none. Load one with prompter.load {{id}} first."),
    )
    .with("state", "empty")
    .with("scripts", ids)
}
