//! `--param key=value` on `gmx ctl source add` and `gmx ctl source set`.
//!
//! A value that reads as JSON is taken as JSON, so `size=44`, `shadow=true`
//! and `items=["One","Two"]` arrive as a number, a boolean and a list.
//! Anything else is a string, so `text=Polls close at ten` needs no quotes
//! beyond the shell's own.

use anyhow::Result;
use serde_json::{Map, Value};

/// The params a list of `key=value` flags names.
pub fn parse(flags: &[String]) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    for flag in flags {
        let Some((key, value)) = flag.split_once('=') else {
            anyhow::bail!("--param {flag:?} has no `=`. Write it as key=value, for example --param size=44");
        };
        let key = key.trim();
        anyhow::ensure!(!key.is_empty(), "--param {flag:?} has no name before the `=`");
        let value = serde_json::from_str::<Value>(value).unwrap_or_else(|_| Value::String(value.replace("\\n", "\n")));
        out.insert(key.to_string(), value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_read_as_json_when_they_are_and_as_words_when_not() {
        let p = parse(&["size=44".into(), "shadow=true".into(), "items=[\"A\",\"B\"]".into(), "text=Ada\\nAnalyst".into()]).unwrap();
        assert_eq!(p["size"], 44);
        assert_eq!(p["shadow"], true);
        assert_eq!(p["items"][1], "B");
        assert_eq!(p["text"], "Ada\nAnalyst");
        assert!(parse(&["size".into()]).unwrap_err().to_string().contains("key=value"));
    }
}
