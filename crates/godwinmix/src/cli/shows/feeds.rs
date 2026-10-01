//! A list of feeds, as a person has it: a CSV export from a headend's channel
//! plan, or one address per line copied out of an email.
//!
//! Two shapes are read. A file whose first line names its columns (`name`,
//! `input`, `program`, `outputs` or `output`, `format`, in any order, others
//! ignored) is a CSV; `gmx-scale feeds` writes one. Anything else is one feed
//! per line: an address alone, or `name,input,program,outputs,format` in that
//! order. Blank lines and lines starting with `#` are skipped. `outputs` holds
//! one or more addresses, separated by spaces or semicolons. `format` is
//! `copy` (the default) or a rendition preset id, for every output of the row.

use anyhow::{bail, Result};
use serde_json::{json, Value};

/// One row, as it will be sent in `show.add_many`.
#[derive(Debug, Clone, PartialEq)]
pub struct Feed {
    /// The file's line number, for an error a person can find.
    pub line: usize,
    pub name: String,
    pub input: String,
    pub program: Option<u16>,
    pub outputs: Vec<String>,
    /// A rendition preset for every output, or None to copy.
    pub format: Option<String>,
}

impl Feed {
    /// The entry `show.add_many` takes for this feed.
    pub fn to_show(&self, compositing: bool) -> Value {
        let mut input = json!({ "uri": self.input });
        if let Some(p) = self.program {
            input["program"] = json!(p);
        }
        let rendition = match &self.format {
            Some(preset) => json!({ "preset": preset }),
            None => Value::Null,
        };
        let outputs: Vec<Value> =
            self.outputs.iter().map(|uri| json!({ "uri": uri, "rendition": rendition })).collect();
        json!({ "name": self.name, "compositing": compositing, "input": input, "outputs": outputs })
    }
}

const COLUMNS: [&str; 5] = ["name", "input", "program", "outputs", "format"];

/// Read a whole file's text.
pub fn parse(text: &str) -> Result<Vec<Feed>> {
    let mut rows = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
        .peekable();
    let header = rows.peek().and_then(|(_, l)| header(l));
    if header.is_some() {
        rows.next();
    }
    let order = header.unwrap_or_else(|| (0..COLUMNS.len()).map(Some).collect());
    let mut feeds = Vec::new();
    for (line, text) in rows {
        feeds.push(row(line, text, &order)?);
    }
    if feeds.is_empty() {
        bail!("the list has no feeds in it. Write one address per line, or a CSV with a name,input header.");
    }
    Ok(feeds)
}

/// Where each of the columns sits, if the line is a
/// header: it names `input` and every cell is a word rather than an address.
fn header(line: &str) -> Option<Vec<Option<usize>>> {
    let cells: Vec<String> = split(line).iter().map(|c| c.to_lowercase()).collect();
    if !cells.iter().any(|c| c == "input") || cells.iter().any(|c| c.contains("://")) {
        return None;
    }
    let at = |col: &str| cells.iter().position(|c| c == col || (col == "outputs" && c == "output"));
    Some(COLUMNS.iter().map(|col| at(col)).collect())
}

fn row(line: usize, text: &str, order: &[Option<usize>]) -> Result<Feed> {
    let cells = split(text);
    let cell = |i: usize| order[i].and_then(|at| cells.get(at)).map(|s| s.trim()).unwrap_or("");
    // One cell on a line is an address alone.
    let (name, input) = if cells.len() == 1 { ("", cells[0].as_str()) } else { (cell(0), cell(1)) };
    if !input.contains(':') {
        bail!("line {line}: {input:?} is not an address. An input is a URI such as udp://@239.1.1.1:5000 or srt://host:9000.");
    }
    let program = match cell(2) {
        "" => None,
        p => Some(p.parse::<u16>().map_err(|_| {
            anyhow::anyhow!("line {line}: program {p:?} is not a number. It is the MPEG-TS program to take, such as 101.")
        })?),
    };
    let outputs = cell(3).split([' ', ';']).filter(|s| !s.is_empty()).map(String::from).collect();
    let name = if name.is_empty() { name_from(input, program) } else { name.to_string() };
    let format = match cell(4) {
        "" | "copy" => None,
        preset => Some(preset.to_string()),
    };
    Ok(Feed { line, name, input: input.to_string(), program, outputs, format })
}

/// A CSV line split on commas, with double quotes around a cell that holds one.
fn split(line: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => cells.push(String::new()),
            c => cells.last_mut().expect("never empty").push(c),
        }
    }
    cells.into_iter().map(|c| c.trim().to_string()).collect()
}

/// A name for a feed that came without one: its host and port, and the
/// program when there is one, so 239.1.1.7:5000 program 3 is `239.1.1.7-5000-p3`.
fn name_from(uri: &str, program: Option<u16>) -> String {
    let rest = uri.split_once("://").map(|(_, r)| r).unwrap_or(uri);
    let host = rest.split(['/', '?']).next().unwrap_or(rest).trim_start_matches('@');
    let base: String = host.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let base = base.trim_matches('-').to_string();
    match program {
        Some(p) => format!("{base}-p{p}"),
        None => base,
    }
}

#[cfg(test)]
#[path = "feeds_tests.rs"]
mod tests;
