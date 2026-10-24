//! `PUT /streamer/api/v3/streams/<name>`: a JSON body in the API's own
//! shape, merged into the stream's block through the lossless writer.
//! Fields left out keep what the file has, and every directive this does not
//! touch stays as it was written, so Flussonic could read the file again.

use crate::syntax::{self, Directive, Node};
use crate::{Document, Error};
use serde_json::{Map, Value};

const FIELDS: &[&str] = &["name", "title", "disabled", "static", "inputs", "transcoder"];

fn bad(msg: impl Into<String>) -> Error {
    Error::Edit(msg.into())
}

fn word(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A bitrate in bit/s as Flussonic writes it: `1500k`, `2m`, `copy`.
fn rate(v: &Value) -> Option<String> {
    match v.as_u64() {
        Some(b) if b % 1_000_000 == 0 => Some(format!("{}m", b / 1_000_000)),
        Some(b) if b % 1000 == 0 => Some(format!("{}k", b / 1000)),
        Some(b) => Some(b.to_string()),
        None => word(v),
    }
}

fn size(s: &Value) -> Option<String> {
    let mut out = format!("{}x{}", s["width"].as_i64()?, s["height"].as_i64()?);
    if let Some(strategy) = s["strategy"].as_str() {
        out.push(':');
        out.push_str(strategy);
    }
    if let Some(bg) = s["background"].as_str() {
        out.push(':');
        out.push_str(bg);
    }
    Some(out)
}

/// The API's transcoder object as the words of a `transcoder` line.
pub fn transcoder_line(t: &Value) -> Result<Vec<String>, Error> {
    if let Some(line) = t.as_str() {
        return Ok(line.split_whitespace().map(String::from).collect());
    }
    let mut args = Vec::new();
    for (k, v) in t["global"].as_object().into_iter().flatten() {
        args.push(format!("{k}={}", word(v).ok_or_else(|| bad(format!("transcoder.global.{k} must be a word or a number")))?));
    }
    for track in t["tracks"].as_array().into_iter().flatten() {
        let (lead, codec) = match track["content"].as_str() {
            Some("video") => ("vb", "vcodec"),
            Some("audio") => ("ab", "acodec"),
            _ => return Err(bad("every transcoder track needs content video or audio")),
        };
        args.push(format!("{lead}={}", rate(&track["bitrate"]).ok_or_else(|| bad("every transcoder track needs a bitrate"))?));
        if let Some(c) = track["codec"].as_str() {
            args.push(format!("{codec}={c}"));
        }
        if let Some(s) = track.get("size").and_then(size) {
            args.push(format!("size={s}"));
        }
        for (key, as_word) in [("preset", "preset"), ("profile", "vprofile"), ("level", "level"), ("bframes", "bframes"), ("channels", "ach"), ("sample_rate", "ar")] {
            if let Some(v) = track.get(key).and_then(word) {
                args.push(format!("{as_word}={v}"));
            }
        }
    }
    Ok(args)
}

fn set_one(body: &mut Vec<Node>, name: &str, args: Option<Vec<String>>) {
    let at = body.iter().position(|n| matches!(n, Node::Directive(d) if d.name == name));
    match (at, args) {
        (Some(i), Some(a)) => body[i] = Node::Directive(Directive::new(name, a)),
        (Some(i), None) => {
            body.remove(i);
        }
        (None, Some(a)) => body.push(Node::Directive(Directive::new(name, a))),
        (None, None) => {}
    }
}

fn set_inputs(body: &mut Vec<Node>, inputs: &Value) -> Result<(), Error> {
    let urls: Vec<String> = inputs
        .as_array()
        .ok_or_else(|| bad("inputs must be a list of {\"url\": ...}"))?
        .iter()
        .map(|i| i["url"].as_str().map(String::from).ok_or_else(|| bad("every input needs a url")))
        .collect::<Result<_, _>>()?;
    let at = body.iter().position(|n| matches!(n, Node::Directive(d) if d.name == "input")).unwrap_or(body.len());
    body.retain(|n| !matches!(n, Node::Directive(d) if d.name == "input"));
    let at = at.min(body.len());
    for (i, url) in urls.into_iter().enumerate() {
        body.insert(at + i, Node::Directive(Directive::new("input", vec![url])));
    }
    Ok(())
}

/// Merge `body` into stream `name`, adding it when the file has none.
pub fn put(doc: &mut Document, name: &str, body: &Value) -> Result<(), Error> {
    let fields: &Map<String, Value> = body.as_object().ok_or_else(|| bad("the body must be a JSON object"))?;
    if let Some(k) = fields.keys().find(|k| !FIELDS.contains(&k.as_str())) {
        return Err(bad(format!("'{k}' cannot be set here. The fields are {}.", FIELDS.join(", "))));
    }
    if fields.get("name").is_some_and(|n| n.as_str() != Some(name)) {
        return Err(bad(format!("the body names another stream than {name}; rename by adding the new one and deleting this")));
    }
    let mut block = match doc.stream_text(name) {
        Some(text) => match syntax::parse(&text)?.into_iter().next() {
            Some(Node::Directive(d)) => d,
            _ => return Err(bad(format!("stream {name} would not read back"))),
        },
        None => Directive::block("stream", vec![name.to_string()], Vec::new()),
    };
    let lines = block.block.get_or_insert_with(Vec::new);
    for (key, directive) in [("title", "title"), ("disabled", "disabled"), ("static", "static")] {
        if let Some(v) = fields.get(key) {
            set_one(lines, directive, word(v).map(|w| vec![w]));
        }
    }
    if let Some(inputs) = fields.get("inputs") {
        set_inputs(lines, inputs)?;
    }
    if let Some(t) = fields.get("transcoder") {
        let args = if t.is_null() { None } else { Some(transcoder_line(t)?) };
        set_one(lines, "transcoder", args);
    }
    doc.put_stream(name, &syntax::print(&[Node::Directive(block)]))
}

#[cfg(test)]
mod tests;
