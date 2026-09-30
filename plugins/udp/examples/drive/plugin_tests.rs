use super::*;

#[test]
fn answers_are_kept_by_id_and_logs_are_ignored() {
    let shared: Shared = Arc::default();
    for line in ["not json", r#"{"method":"initialize","params":{}}"#, r#"{"id":3,"result":{"ok":true}}"#, r#"{"id":4,"method":"log"}"#] {
        take(line, &shared);
    }
    assert!(wait(&shared, |a| a.hello.take()).is_some());
    assert_eq!(wait(&shared, |a| a.by_id.remove(&3)).unwrap()["result"]["ok"], true);
    assert!(shared.0.lock().unwrap().by_id.is_empty(), "a request from the plugin is not an answer");
}

#[test]
fn the_summary_keeps_its_order() {
    let text = ordered_json(&[("seconds", json!(60)), ("programs", json!({ "a": [1] })), ("resumed", Value::Null)]);
    assert_eq!(text, "{\n \"seconds\": 60,\n \"programs\": {\n  \"a\": [\n   1\n  ]\n },\n \"resumed\": null\n}");
}
