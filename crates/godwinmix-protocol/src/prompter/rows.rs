//! A script broken into rows, the unit every prompter position is counted in.
//!
//! The rule is simple on purpose, so a client in any language can repeat it:
//! each line as written is broken at spaces into rows of at most `width`
//! characters, a word longer than that is cut, a blank line is one empty row,
//! and a line starting with `#` is a marker whose words are its rows. The
//! asterisks of `*italic*` and `**bold**` do not count towards the width,
//! because a screen does not draw them.

use super::Marker;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One row of a script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Row {
    /// The words, with any asterisks still in them.
    pub text: String,
    /// The line it came from, counting from 0.
    pub line: u32,
    /// True for the rows of a marker line, drawn as a heading.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub marker: bool,
}

/// The words of a marker line, or None when the line is not one.
fn marker_words(line: &str) -> Option<&str> {
    line.trim().strip_prefix('#').map(|rest| rest.trim_start_matches('#').trim())
}

/// How wide a piece of text is drawn, in characters.
fn drawn(text: &str) -> usize {
    text.chars().filter(|c| *c != '*').count()
}

/// The script as rows at `width` characters.
pub fn rows(text: &str, width: u32) -> Vec<Row> {
    let width = width.max(1) as usize;
    let mut out = Vec::new();
    for (n, line) in text.split('\n').enumerate() {
        let line = line.trim_end_matches('\r');
        let (words, marker) = match marker_words(line) {
            Some(words) => (words, true),
            None => (line.trim(), false),
        };
        let mut pieces = wrap(words, width);
        if pieces.is_empty() {
            pieces.push(String::new());
        }
        out.extend(pieces.into_iter().map(|text| Row { text, line: n as u32, marker }));
    }
    out
}

/// One line's words, packed into rows.
fn wrap(words: &str, width: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    for word in words.split_whitespace() {
        for piece in cut(word, width) {
            let fits = row.is_empty() || drawn(&row) + 1 + drawn(&piece) <= width;
            if !fits {
                rows.push(std::mem::take(&mut row));
            }
            if !row.is_empty() {
                row.push(' ');
            }
            row.push_str(&piece);
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

/// A word longer than a row, cut into pieces that fit.
fn cut(word: &str, width: usize) -> Vec<String> {
    if drawn(word) <= width {
        return vec![word.to_string()];
    }
    let mut pieces = vec![String::new()];
    for c in word.chars() {
        let last = pieces.last_mut().expect("there is always a piece");
        if c != '*' && drawn(last) == width {
            pieces.push(c.to_string());
        } else {
            last.push(c);
        }
    }
    pieces
}

/// Every marker in a script, with the row it starts on at `width`.
pub fn markers(rows: &[Row]) -> Vec<Marker> {
    let mut out = Vec::new();
    let mut last_line = None;
    for (at, row) in rows.iter().enumerate() {
        if !row.marker || last_line == Some(row.line) {
            continue;
        }
        last_line = Some(row.line);
        let words: Vec<&str> =
            rows.iter().filter(|r| r.marker && r.line == row.line).map(|r| r.text.as_str()).collect();
        let name = words.join(" ").replace('*', "");
        let name = if name.is_empty() { format!("marker {}", out.len() + 1) } else { name };
        out.push(Marker { name, row: at as u32 });
    }
    out
}
