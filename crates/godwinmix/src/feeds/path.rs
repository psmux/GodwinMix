//! The path `select` takes, and what it picks out of a document.
//!
//! Dotted keys with array indexes: `data.matches[0].home.score`. `[-1]` is
//! the last element, `[]` (or `[*]`) every element, so `items[].title` is a
//! list of titles. `["a key.with dots"]` is a key the dotted form cannot
//! spell. A path starting `/` is a JSON pointer. Empty, `.` or `$` is the
//! whole document.
//!
//! A path that picks nothing says where it stopped and what was there, which
//! is the whole difference between guessing and reading.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// An object's key, or an array's index written as a pointer token.
    Key(String),
    Index(i64),
    Each,
}

/// Where a path stopped and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Miss {
    /// The part of the path that did select something.
    pub at: String,
    pub reason: String,
    /// The keys of what is at `at`, when it is an object.
    pub keys: Vec<String>,
}

pub fn parse(text: &str) -> Result<Vec<Step>, String> {
    let text = text.trim();
    if let Some(pointer) = text.strip_prefix('/') {
        return Ok(pointer.split('/').map(|t| Step::Key(t.replace("~1", "/").replace("~0", "~"))).collect());
    }
    let text = text.strip_prefix('$').unwrap_or(text);
    let mut steps = Vec::new();
    let mut chars = text.chars().peekable();
    let mut key = String::new();
    while let Some(c) = chars.next() {
        match c {
            '.' => flush(&mut key, &mut steps),
            '[' => {
                flush(&mut key, &mut steps);
                let mut inside = String::new();
                let mut quote: Option<char> = None;
                loop {
                    let Some(c) = chars.next() else {
                        return Err(format!("`{text}` has a `[` that is never closed"));
                    };
                    match (quote, c) {
                        (None, ']') => break,
                        (None, '"' | '\'') if inside.is_empty() => quote = Some(c),
                        (Some(q), c) if c == q => quote = None,
                        _ => inside.push(c),
                    }
                }
                steps.push(bracket(text, &inside)?);
            }
            _ => key.push(c),
        }
    }
    flush(&mut key, &mut steps);
    Ok(steps)
}

fn flush(key: &mut String, steps: &mut Vec<Step>) {
    let k = std::mem::take(key);
    let k = k.trim();
    if !k.is_empty() {
        steps.push(Step::Key(k.to_string()));
    }
}

fn bracket(text: &str, inside: &str) -> Result<Step, String> {
    let t = inside.trim();
    if t.is_empty() || t == "*" {
        return Ok(Step::Each);
    }
    if let Ok(n) = t.parse::<i64>() {
        return Ok(Step::Index(n));
    }
    if inside.is_empty() {
        return Err(format!("`{text}` has an empty `[]` key"));
    }
    Ok(Step::Key(inside.to_string()))
}

/// Write steps back as a path a person reads.
pub fn show(steps: &[Step]) -> String {
    let mut out = String::new();
    for step in steps {
        match step {
            Step::Key(k) if k.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(k);
            }
            Step::Key(k) => out.push_str(&format!("[\"{k}\"]")),
            Step::Index(i) => out.push_str(&format!("[{i}]")),
            Step::Each => out.push_str("[]"),
        }
    }
    out
}

/// Pick out of `doc`. After a `[]` the rest of the path runs on each element
/// and the answer is a list; elements the rest picks nothing from are left
/// out, and a list of lists is made one list.
pub fn select(doc: &Value, steps: &[Step]) -> Result<Value, Miss> {
    walk(doc, steps, 0, steps)
}

fn walk(here: &Value, rest: &[Step], depth: usize, all: &[Step]) -> Result<Value, Miss> {
    let Some((step, rest)) = rest.split_first() else {
        return Ok(here.clone());
    };
    let at = || show(&all[..depth]);
    match (step, here) {
        (Step::Each, Value::Array(items)) => each(items, rest, depth, all),
        (Step::Each, other) => Err(miss(at(), format!("is {}, not a list, so `[]` has nothing to go through", kind(other)), other)),
        (Step::Index(i), Value::Array(items)) => {
            let n = items.len() as i64;
            let at_index = if *i < 0 { n + i } else { *i };
            match items.get(at_index.max(0) as usize).filter(|_| at_index >= 0) {
                Some(v) => walk(v, rest, depth + 1, all),
                None => Err(miss(at(), format!("has {n} elements, so [{i}] is past the end"), here)),
            }
        }
        (Step::Key(k), Value::Array(items)) if k.parse::<usize>().is_ok() => match items.get(k.parse::<usize>().unwrap_or(0)) {
            Some(v) => walk(v, rest, depth + 1, all),
            None => Err(miss(at(), format!("has {} elements, so {k} is past the end", items.len()), here)),
        },
        (Step::Key(k), Value::Object(map)) => match map.get(k) {
            Some(v) => walk(v, rest, depth + 1, all),
            None => Err(miss(at(), format!("has no key `{k}`"), here)),
        },
        (step, other) => Err(miss(at(), format!("is {}, so `{}` cannot go into it", kind(other), show(std::slice::from_ref(step))), other)),
    }
}

fn each(items: &[Value], rest: &[Step], depth: usize, all: &[Step]) -> Result<Value, Miss> {
    let mut out = Vec::new();
    let mut first_miss = None;
    let spreads = rest.contains(&Step::Each);
    for item in items {
        match walk(item, rest, depth + 1, all) {
            Ok(Value::Array(inner)) if spreads => out.extend(inner),
            Ok(v) => out.push(v),
            Err(m) => {
                first_miss.get_or_insert(m);
            }
        }
    }
    match first_miss {
        Some(m) if out.is_empty() => Err(m),
        _ => Ok(Value::Array(out)),
    }
}

fn miss(at: String, reason: String, here: &Value) -> Miss {
    let keys = match here {
        Value::Object(map) => map.keys().cloned().collect(),
        _ => Vec::new(),
    };
    Miss { at, reason, keys }
}

/// What a value is, for a sentence.
pub fn kind(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(_) => "a true or false".into(),
        Value::Number(n) => format!("the number {n}"),
        Value::String(_) => "a string".into(),
        Value::Array(a) => format!("a list of {}", a.len()),
        Value::Object(_) => "an object".into(),
    }
}

impl Miss {
    /// One sentence naming the path, where it stopped and what was there.
    pub fn sentence(&self, path: &str) -> String {
        let place = if self.at.is_empty() { "the document".to_string() } else { format!("`{}`", self.at) };
        let mut s = format!("`{path}` selects nothing: {place} {}.", self.reason);
        if !self.keys.is_empty() {
            let shown: Vec<&str> = self.keys.iter().take(20).map(String::as_str).collect();
            s.push_str(&format!(" It has: {}.", shown.join(", ")));
        }
        s
    }
}

#[cfg(test)]
#[path = "path_tests.rs"]
mod tests;
