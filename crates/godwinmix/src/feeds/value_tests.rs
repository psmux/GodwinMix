use super::*;
use serde_json::json;

fn sel(select: &str, template: Option<&str>, limit: Option<u32>, join: Option<&str>) -> Selection {
    Selection { select: select.into(), template: template.map(Into::into), limit, join: join.map(Into::into) }
}

#[test]
fn a_template_combines_fields_of_one_value() {
    let doc = json!({ "match": { "home": "Leeds", "away": "Hull", "home_score": 2, "away_score": 0 } });
    let (_, out) = compute(&doc, &sel("match", Some("{home} {home_score} : {away_score} {away}"), None, None)).unwrap();
    assert_eq!(out, json!("Leeds 2 : 0 Hull"));
}

#[test]
fn a_list_is_cut_filled_and_joined() {
    let doc = json!({ "items": [ { "title": "One" }, { "title": "Two" }, { "title": "Three" } ] });
    let (picked, out) = compute(&doc, &sel("items[].title", None, Some(2), None)).unwrap();
    assert_eq!(picked, json!(["One", "Two", "Three"]));
    assert_eq!(out, json!(["One", "Two"]));
    let (_, out) = compute(&doc, &sel("items", Some("* {title}"), None, Some(" / "))).unwrap();
    assert_eq!(out, json!("* One / * Two / * Three"));
}

#[test]
fn braces_double_up_and_a_hole_can_be_the_whole_element() {
    let doc = json!({ "n": [1, 2] });
    let (_, out) = compute(&doc, &sel("n", Some("{{{}}}"), None, None)).unwrap();
    assert_eq!(out, json!(["{1}", "{2}"]));
}

#[test]
fn a_hole_missing_from_some_elements_is_empty_and_from_all_is_refused() {
    let doc = json!({ "rows": [ { "Name": "Ada", "Role": "Host" }, { "Name": "Grace" } ] });
    let (_, out) = compute(&doc, &sel("rows", Some("{Name}, {Role}"), None, None)).unwrap();
    assert_eq!(out, json!(["Ada, Host", "Grace, "]));
    let err = compute(&doc, &sel("rows", Some("{Nmae}"), None, None)).unwrap_err();
    assert!(err.message.contains("{Nmae} is in none of the 2 elements") && err.message.contains("It has: Name, Role."), "{}", err.message);
}

#[test]
fn a_path_that_selects_nothing_names_the_keys() {
    let doc = json!({ "title": "x", "items": [] });
    let err = compute(&doc, &sel("headlines", None, None, None)).unwrap_err();
    assert_eq!(err.miss.unwrap().keys, vec!["items", "title"]);
}

#[test]
fn numbers_and_nulls_read_as_words() {
    assert_eq!(words(&json!(2.5)), "2.5");
    assert_eq!(words(&json!(3)), "3");
    assert_eq!(words(&json!(null)), "");
    assert_eq!(words(&json!(["a", 1])), "a, 1");
}
