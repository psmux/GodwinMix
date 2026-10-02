//! A body into a document, whatever it was written in.
//!
//! `auto` goes by the content type first and the first byte second, which is
//! right for every feed tried: a JSON API says `application/json`, a feed says
//! `application/rss+xml` or starts `<`, and a published sheet says `text/csv`.

use godwinmix_protocol::feeds::FeedFormat;
use serde_json::{json, Value};

/// Read `body` as `format`, and say what it was read as.
pub fn read(body: &[u8], content_type: Option<&str>, format: FeedFormat) -> Result<(FeedFormat, Value), String> {
    let format = match format {
        FeedFormat::Auto | FeedFormat::Sse => detect(body, content_type),
        other => other,
    };
    let text = String::from_utf8_lossy(body);
    let doc = match format {
        FeedFormat::Json => serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("it is not JSON ({e})"))?,
        FeedFormat::Rss => super::rss::read(&text)?,
        FeedFormat::Csv => super::csv::read(&text)?,
        _ => text_doc(&text),
    };
    Ok((format, doc))
}

/// A message that is JSON is the document; one that is not is words.
pub fn message(text: &str) -> Value {
    match serde_json::from_str::<Value>(text.trim()) {
        Ok(v) => v,
        Err(_) => text_doc(text),
    }
}

fn text_doc(text: &str) -> Value {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    json!({ "text": text.trim(), "lines": lines })
}

fn detect(body: &[u8], content_type: Option<&str>) -> FeedFormat {
    let ct = content_type.unwrap_or_default().to_ascii_lowercase();
    if ct.contains("json") {
        return FeedFormat::Json;
    }
    if ct.contains("xml") || ct.contains("rss") || ct.contains("atom") {
        return FeedFormat::Rss;
    }
    if ct.contains("csv") || ct.contains("comma-separated") {
        return FeedFormat::Csv;
    }
    let first = body.iter().copied().find(|b| !b.is_ascii_whitespace() && *b != 0xEF && *b != 0xBB && *b != 0xBF);
    match first {
        Some(b'{') | Some(b'[') => FeedFormat::Json,
        Some(b'<') => FeedFormat::Rss,
        _ if body.contains(&b',') => FeedFormat::Csv,
        _ => FeedFormat::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_content_type_wins_and_the_first_byte_decides_otherwise() {
        assert_eq!(detect(b"a,b", Some("application/json; charset=utf-8")), FeedFormat::Json);
        assert_eq!(detect(b"{}", Some("text/csv")), FeedFormat::Csv);
        assert_eq!(detect(b"  [1]", None), FeedFormat::Json);
        assert_eq!(detect(b"<rss/>", Some("text/plain")), FeedFormat::Rss);
        assert_eq!(detect(b"Name,Title\nAda,Host", Some("text/plain")), FeedFormat::Csv);
        assert_eq!(detect(b"Just words", None), FeedFormat::Text);
    }

    #[test]
    fn json_that_does_not_parse_says_so() {
        let err = read(b"{\"a\":", None, FeedFormat::Json).unwrap_err();
        assert!(err.starts_with("it is not JSON"), "{err}");
    }

    #[test]
    fn a_message_that_is_not_json_is_words() {
        assert_eq!(message("{\"score\":3}"), json!({ "score": 3 }));
        assert_eq!(message("GOAL\n"), json!({ "text": "GOAL", "lines": ["GOAL"] }));
    }
}
