//! From a fetched document to the value a binding writes.
//!
//! `select` picks, `limit` cuts a list, `template` turns each picked thing
//! into words, `join` makes a list one string. Each step is optional, and
//! with none of them the whole document is the value.

use super::path::{self, Miss, Step};
use godwinmix_protocol::feeds::Selection;
use serde_json::Value;

/// Why a selection gave no value.
#[derive(Debug, Clone, PartialEq)]
pub struct NoValue {
    pub message: String,
    /// Where the path stopped, and the keys there.
    pub miss: Option<Miss>,
}

impl NoValue {
    fn plain(message: String) -> Self {
        NoValue { message, miss: None }
    }
}

/// What `select` picked, and what a binding would write.
pub fn compute(doc: &Value, sel: &Selection) -> Result<(Value, Value), NoValue> {
    let steps = path::parse(&sel.select).map_err(NoValue::plain)?;
    let picked = path::select(doc, &steps).map_err(|m| NoValue { message: m.sentence(&sel.select), miss: Some(m) })?;
    let template = match sel.template.as_deref().filter(|t| !t.is_empty()) {
        Some(t) => Some(Template::parse(t).map_err(NoValue::plain)?),
        None => None,
    };
    let out = match &picked {
        Value::Array(items) => {
            let limit = sel.limit.filter(|n| *n > 0).map(|n| n as usize).unwrap_or(usize::MAX);
            let items: Vec<Value> = items.iter().take(limit).cloned().collect();
            let items = match &template {
                Some(t) => t.fill_each(&items)?.into_iter().map(Value::String).collect(),
                None => items,
            };
            match &sel.join {
                Some(sep) => Value::String(items.iter().map(words).collect::<Vec<_>>().join(sep)),
                None => Value::Array(items),
            }
        }
        one => match &template {
            Some(t) => Value::String(t.fill_one(one)?),
            None => one.clone(),
        },
    };
    Ok((picked, out))
}

/// A value as the words it puts on screen.
pub fn words(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(words).collect::<Vec<_>>().join(", "),
        Value::Object(_) => v.to_string(),
        other => other.to_string(),
    }
}

enum Piece {
    Text(String),
    Hole(String, Vec<Step>),
}

/// Words with `{path}` holes. `{{` and `}}` are a brace.
struct Template(Vec<Piece>);

impl Template {
    fn parse(text: &str) -> Result<Template, String> {
        let mut pieces = Vec::new();
        let mut text_buf = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    text_buf.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    text_buf.push('}');
                }
                '{' => {
                    let hole: String = chars.by_ref().take_while(|c| *c != '}').collect();
                    pieces.push(Piece::Text(std::mem::take(&mut text_buf)));
                    let name = hole.trim().to_string();
                    let steps = if name == "." { Vec::new() } else { path::parse(&name)? };
                    pieces.push(Piece::Hole(name, steps));
                }
                c => text_buf.push(c),
            }
        }
        pieces.push(Piece::Text(text_buf));
        Ok(Template(pieces))
    }

    /// Fill from one value. A hole that picks nothing is refused.
    fn fill_one(&self, v: &Value) -> Result<String, NoValue> {
        let (out, missed) = self.fill(v);
        match missed.into_iter().next() {
            Some((name, m)) => Err(NoValue { message: format!("the template's {{{name}}}: {}", m.sentence(&name)), miss: Some(m) }),
            None => Ok(out),
        }
    }

    /// Fill from each element. A hole that is missing from some elements is
    /// empty in those; one missing from every element is refused.
    fn fill_each(&self, items: &[Value]) -> Result<Vec<String>, NoValue> {
        let mut out = Vec::with_capacity(items.len());
        let mut found_any = std::collections::BTreeSet::new();
        let mut first_miss = None;
        for item in items {
            let (words, missed) = self.fill(item);
            for piece in &self.0 {
                if let Piece::Hole(name, _) = piece {
                    if !missed.iter().any(|(n, _)| n == name) {
                        found_any.insert(name.clone());
                    }
                }
            }
            if first_miss.is_none() {
                first_miss = missed.into_iter().next();
            }
            out.push(words);
        }
        match first_miss {
            Some((name, m)) if !found_any.contains(&name) => Err(NoValue {
                message: format!("the template's {{{name}}} is in none of the {} elements: {}", items.len(), m.sentence(&name)),
                miss: Some(m),
            }),
            _ => Ok(out),
        }
    }

    fn fill(&self, v: &Value) -> (String, Vec<(String, Miss)>) {
        let mut out = String::new();
        let mut missed = Vec::new();
        for piece in &self.0 {
            match piece {
                Piece::Text(t) => out.push_str(t),
                Piece::Hole(name, steps) => match path::select(v, steps) {
                    Ok(found) => out.push_str(&words(&found)),
                    Err(m) => missed.push((name.clone(), m)),
                },
            }
        }
        (out, missed)
    }
}

#[cfg(test)]
#[path = "value_tests.rs"]
mod tests;
