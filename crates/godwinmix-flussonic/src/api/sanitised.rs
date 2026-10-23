//! `GET /streamer/api/v3/config`: the typed view of the file, with no
//! password in it. The credentials' passwords are never serialised, and a
//! password inside an input or push URL is replaced.

use crate::import::redact;
use crate::model::Config;
use serde_json::Value;

fn redact_urls(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, val) in m.iter_mut() {
                match val {
                    Value::String(s) if k == "url" || k == "pushes" => *s = redact(s),
                    _ => redact_urls(val),
                }
            }
        }
        Value::Array(a) => {
            for item in a {
                match item {
                    Value::String(s) if s.contains("://") => *s = redact(s),
                    _ => redact_urls(item),
                }
            }
        }
        _ => {}
    }
}

pub fn config(cfg: &Config) -> Value {
    let mut v = serde_json::to_value(cfg).unwrap_or_default();
    redact_urls(&mut v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn no_password_leaves() {
        let text = "edit_auth admin topsecret;\nstream A {\n  input rtmp://bob:hunter2@h/app/a;\n  push rtmp://x:pushpass@y/live/k;\n}\n";
        let v = config(&Document::parse(text).unwrap().config);
        let s = v.to_string();
        assert!(!s.contains("topsecret") && !s.contains("hunter2") && !s.contains("pushpass"), "{s}");
        assert!(s.contains("bob:***@h") && s.contains("\"admin\""), "{s}");
    }
}
