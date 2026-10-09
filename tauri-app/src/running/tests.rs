use super::*;
use serde_json::json;

fn lines(things: &[Thing]) -> Vec<&str> {
    things.iter().map(|t| t.line.as_str()).collect()
}

#[test]
fn a_live_stream_a_recording_and_a_channel_are_said_in_words() {
    let outputs = json!([
        {"id": "youtube", "uri_host": "rtmp://a.rtmp.youtube.com/…", "has_key": true, "state": "live", "live_secs": 6983},
        {"id": "recording-1", "type": "record/output", "state": "live", "recording_secs": 65},
        {"id": "facebook", "uri_host": "rtmps://live-api-s.facebook.com:443/…", "state": "stopped"},
        {"id": "twitch", "uri_host": "rtmp://live.twitch.tv/…", "has_key": false, "state": "reconnecting"}
    ]);
    let channels = json!({"channels": [{
        "id": "church", "name": "Church", "enabled": true,
        "streams": [{"name": "main", "state": "live", "from": "192.168.1.20"}],
        "destinations": [
            {"id": "yt", "label": "YouTube", "enabled": true, "state": "live"},
            {"id": "kick", "label": "Kick", "enabled": false, "state": "off"}
        ]
    }]});
    let things = describe(&outputs, &channels);
    assert_eq!(
        lines(&things),
        [
            "Streaming to YouTube, live for 1:56:23",
            "Recording, for 1:05",
            "Channel Church sending on to YouTube",
            "Channel Church receiving from 192.168.1.20",
        ]
    );
    assert_eq!(things[0].stop.as_ref().unwrap().path, "/api/v1/outputs/youtube/stop");
    assert_eq!(things[1].stop.as_ref().unwrap().method, "DELETE");
    assert_eq!(things[2].stop.as_ref().unwrap().body, Some(json!({"destination": "yt", "enabled": false})));
    assert!(!things[3].outgoing && things[3].stop.is_none(), "an encoder coming in is not stopped by Stop all");
}

#[test]
fn nothing_running_is_an_empty_list_whatever_shape_the_answer_has() {
    assert!(describe(&json!([]), &json!({"channels": []})).is_empty());
    assert!(describe(&json!({"outputs": []}), &Value::Null).is_empty());
    assert!(describe(&Value::Null, &Value::Null).is_empty());
    let stopped = json!([{"id": "yt", "uri_host": "rtmp://a.rtmp.youtube.com/…", "state": "stopped"}]);
    assert!(describe(&stopped, &Value::Null).is_empty(), "a stopped destination is not running");
}

#[test]
fn a_destination_still_dialling_counts_and_says_so() {
    let dialling = json!([{"id": "own-server", "uri_host": "rtmp://10.0.0.5/…", "has_key": true, "state": "connecting"}]);
    assert_eq!(lines(&describe(&dialling, &Value::Null)), ["Streaming to own-server, connecting"]);
}

#[test]
fn the_clock_reads_like_the_page() {
    assert_eq!(clock(65), "1:05");
    assert_eq!(clock(6983), "1:56:23");
}
