//! RSS 2.0, RSS 1.0 and Atom, read into one shape:
//!
//! ```json
//! { "title": "...", "link": "...", "items": [
//!     { "title", "link", "summary", "published", "id", "author" } ] }
//! ```
//!
//! so `items[].title` is the headlines whichever kind of feed it was. A
//! summary has its HTML taken out, because a ticker shows words.

use super::xml::{self, Node};
use serde_json::{json, Map, Value};

pub fn read(text: &str) -> Result<Value, String> {
    let root = xml::parse(text)?;
    let (channel, entries): (Option<&Node>, Vec<&Node>) = match root.local() {
        "rss" => {
            let channel = root.child("channel");
            (channel, channel.map(|c| c.all("item").collect()).unwrap_or_default())
        }
        "RDF" => (root.child("channel"), root.all("item").collect()),
        "feed" => (Some(&root), root.all("entry").collect()),
        other => return Err(format!("it is XML, but its root is <{other}> rather than <rss> or <feed>, so it is not a news feed")),
    };
    let mut doc = Map::new();
    if let Some(c) = channel {
        doc.insert("title".into(), json!(plain(c.child("title"))));
        doc.insert("link".into(), json!(link(c)));
        doc.insert("description".into(), json!(clean(&text_of(c.child("description").or_else(|| c.child("subtitle"))))));
    }
    doc.insert("items".into(), Value::Array(entries.into_iter().map(item).collect()));
    Ok(Value::Object(doc))
}

fn item(n: &Node) -> Value {
    let summary = n.child("description").or_else(|| n.child("summary")).or_else(|| n.child("content"));
    let published = ["pubDate", "published", "updated", "date"].iter().find_map(|k| n.child(k));
    let author = n.child("author").map(|a| a.child("name").unwrap_or(a)).or_else(|| n.child("creator"));
    json!({
        "title": plain(n.child("title")),
        "link": link(n),
        "summary": clean(&text_of(summary)),
        "published": plain(published),
        "id": plain(n.child("guid").or_else(|| n.child("id"))),
        "author": plain(author),
    })
}

/// RSS keeps the address as text; Atom as `href`, the `alternate` one first.
fn link(n: &Node) -> String {
    let links: Vec<&Node> = n.all("link").collect();
    let atom = links
        .iter()
        .find(|l| l.attr("href").is_some() && matches!(l.attr("rel"), None | Some("alternate")))
        .or_else(|| links.iter().find(|l| l.attr("href").is_some()));
    match atom {
        Some(l) => l.attr("href").unwrap_or_default().to_string(),
        None => links.first().map(|l| l.text.trim().to_string()).unwrap_or_default(),
    }
}

fn text_of(n: Option<&Node>) -> String {
    n.map(|n| n.text.clone()).unwrap_or_default()
}

fn plain(n: Option<&Node>) -> String {
    clean(&text_of(n))
}

/// Tags out, entities read, white space made single spaces.
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    xml::decode(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0"?>
<!-- a comment -->
<rss version="2.0" xmlns:dc="http://purl.org/dc/elements/1.1/"><channel>
<title>Town News</title><link>https://news.example/</link>
<item><title>Polls close at ten &amp; counting starts</title><link>https://news.example/1</link>
<description><![CDATA[<p>The <b>count</b> begins.</p>]]></description><pubDate>Fri, 02 Oct 2026 20:00:00 GMT</pubDate>
<guid>n1</guid><dc:creator>Ada</dc:creator></item>
<item><title>Rain later</title><description>&lt;p&gt;Bring a coat&lt;/p&gt;</description></item>
</channel></rss>"#;

    const ATOM: &str = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Scores</title>
<link rel="self" href="https://s.example/feed"/><link href="https://s.example/"/>
<entry><title type="html">Leeds 2 &#8226; 0 Hull</title><link rel="alternate" href="https://s.example/m1"/>
<id>tag:s,1</id><updated>2026-10-02T19:00:00Z</updated><summary>Full time</summary><author><name>Desk</name></author></entry>
</feed>"#;

    #[test]
    fn rss_items_are_read_with_their_html_taken_out() {
        let doc = read(RSS).unwrap();
        assert_eq!(doc["title"], "Town News");
        assert_eq!(doc["items"][0]["title"], "Polls close at ten & counting starts");
        assert_eq!(doc["items"][0]["summary"], "The count begins.");
        assert_eq!(doc["items"][0]["author"], "Ada");
        assert_eq!(doc["items"][0]["link"], "https://news.example/1");
        assert_eq!(doc["items"][1]["summary"], "Bring a coat");
    }

    #[test]
    fn atom_entries_read_into_the_same_shape() {
        let doc = read(ATOM).unwrap();
        assert_eq!(doc["link"], "https://s.example/");
        let e = &doc["items"][0];
        assert_eq!(e["title"], "Leeds 2 \u{2022} 0 Hull");
        assert_eq!(e["link"], "https://s.example/m1");
        assert_eq!(e["published"], "2026-10-02T19:00:00Z");
        assert_eq!(e["author"], "Desk");
    }

    #[test]
    fn xml_that_is_not_a_feed_says_so() {
        assert!(read("<html><body/></html>").unwrap_err().contains("<html>"));
    }
}
