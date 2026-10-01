use super::*;
use serde_json::json;

#[test]
fn the_worst_show_is_on_top_with_its_alarm_and_its_age() {
    let stats = json!({ "shows": [
        { "id": "bbc-one", "health": { "state": "ok", "alarms": [] },
          "input": { "kbps": 8100, "fps": 25.0, "width": 1920, "height": 1080,
                     "video_codec": "h264", "audio_codec": "aac", "cc_errors": 0 },
          "outputs": [{ "id": "out", "state": "live", "kbps": 8000 }] },
        { "id": "bbc-two", "health": { "state": "alarm",
            "alarms": [{ "kind": "no-input", "since_ms": 75000, "detail": "" }] },
          "input": {}, "outputs": [{ "id": "out", "state": "waiting" }] }
    ]});
    let text = render(&stats);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "2 shows: 1 ok, 0 warning, 1 alarm, 0 off");
    assert!(lines[2].starts_with("bbc-two") && lines[2].ends_with("no-input 1m"), "{text}");
    assert!(lines[3].contains("1920x1080") && lines[3].contains("h264/aac"), "{text}");
    assert!(lines[3].contains("1/1"), "{text}");
}

#[test]
fn an_empty_answer_is_a_tally_of_nothing() {
    assert!(render(&json!({})).starts_with("0 shows"));
}
