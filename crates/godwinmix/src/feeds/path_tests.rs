use super::*;
use serde_json::json;

fn pick(doc: &Value, path: &str) -> Result<Value, Miss> {
    select(doc, &parse(path).unwrap())
}

#[test]
fn dotted_keys_and_indexes_reach_into_a_document() {
    let doc = json!({ "data": { "matches": [ { "home": { "score": 2 } }, { "home": { "score": 5 } } ] } });
    assert_eq!(pick(&doc, "data.matches[0].home.score").unwrap(), json!(2));
    assert_eq!(pick(&doc, "data.matches[-1].home.score").unwrap(), json!(5));
    assert_eq!(pick(&doc, "$.data.matches[1].home").unwrap(), json!({ "score": 5 }));
    assert_eq!(pick(&doc, "/data/matches/1/home/score").unwrap(), json!(5));
    assert_eq!(pick(&doc, "").unwrap(), doc);
}

#[test]
fn every_element_is_a_list_and_lists_of_lists_are_flattened() {
    let doc = json!({ "items": [ { "title": "a", "tags": ["x", "y"] }, { "title": "b", "tags": ["z"] }, { "link": "c" } ] });
    assert_eq!(pick(&doc, "items[].title").unwrap(), json!(["a", "b"]));
    assert_eq!(pick(&doc, "items[*].tags[]").unwrap(), json!(["x", "y", "z"]));
    assert_eq!(pick(&json!({ "items": [] }), "items[].title").unwrap(), json!([]));
}

#[test]
fn a_quoted_key_holds_dots_and_spaces() {
    let doc = json!({ "Home team": { "a.b": 1 } });
    assert_eq!(pick(&doc, "[\"Home team\"]['a.b']").unwrap(), json!(1));
    assert_eq!(show(&parse("[\"Home team\"].x[2][]").unwrap()), "[\"Home team\"].x[2][]");
}

#[test]
fn a_miss_says_where_it_stopped_and_what_was_there() {
    let doc = json!({ "items": [ { "title": "a", "link": "l" } ], "title": "News" });
    let m = pick(&doc, "items[0].titel").unwrap_err();
    assert_eq!(m.at, "items[0]");
    assert_eq!(m.keys, vec!["link", "title"]);
    let s = m.sentence("items[0].titel");
    assert!(s.contains("has no key `titel`") && s.contains("It has: link, title."), "{s}");
    let m = pick(&doc, "items[4]").unwrap_err();
    assert!(m.reason.contains("has 1 elements"), "{m:?}");
    let m = pick(&doc, "title.x").unwrap_err();
    assert!(m.reason.contains("is a string"), "{m:?}");
    let m = pick(&doc, "nothing").unwrap_err();
    assert!(m.sentence("nothing").contains("the document has no key `nothing`. It has: items, title."));
}

#[test]
fn an_unclosed_bracket_is_refused() {
    assert!(parse("items[0").unwrap_err().contains("never closed"));
}
