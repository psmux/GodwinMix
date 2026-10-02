use super::*;

#[test]
fn every_special_character_is_escaped_and_comes_back() {
    let raw = r#"Tom & Jerry <live> "now" it's"#;
    let e = escape(raw);
    assert_eq!(e, "Tom &amp; Jerry &lt;live&gt; &quot;now&quot; it&apos;s");
    assert_eq!(unescape(&e), raw);
    assert_eq!(unescape("&#65;&#x42; & alone"), "AB & alone");
}

#[test]
fn tags_are_found_by_name_and_a_quoted_bracket_does_not_end_one() {
    let doc = r#"<svg><text x="1" data-note="a>b">One</text><textPath/><text>Two</text></svg>"#;
    let found = tags(doc, "text");
    assert_eq!(found.len(), 2, "textPath is another element");
    assert_eq!(found[0].text, r#"<text x="1" data-note="a>b">"#);
    assert_eq!(attr(found[0].text, "data-note").as_deref(), Some("a>b"));
    assert_eq!(attr(found[0].text, "x").as_deref(), Some("1"));
    assert_eq!(attr(found[0].text, "note"), None, "a name inside another is not it");
}

#[test]
fn an_attribute_is_replaced_where_it_is_or_added_at_the_end() {
    assert_eq!(set_attr(r#"<text x="1" y='2'>"#, "y", "5"), r#"<text x="1" y='5'>"#);
    assert_eq!(set_attr(r#"<rect x="1"/>"#, "id", "a&b"), r#"<rect x="1" id="a&amp;b"/>"#);
}
