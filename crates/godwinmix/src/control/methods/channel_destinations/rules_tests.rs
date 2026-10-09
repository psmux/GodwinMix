use super::*;

fn add_req(platform: &str) -> AddDestinationRequest {
    AddDestinationRequest { id: "sunday".into(), platform: platform.into(), ..Default::default() }
}

fn field(e: &RpcError) -> &str {
    e.data["field"].as_str().unwrap_or("")
}

#[test]
fn youtube_takes_the_platform_server_and_needs_only_the_key() {
    let mut list = Vec::new();
    let err = add(&mut list, &add_req("youtube")).unwrap_err();
    assert_eq!(field(&err), "key");
    assert!(err.message.contains("YouTube needs its stream key"), "{}", err.message);

    let req = AddDestinationRequest { key: Some(" abcd-1234\n".into()), ..add_req("youtube") };
    assert_eq!(add(&mut list, &req).unwrap(), "youtube");
    assert_eq!(list[0].url(), "rtmp://a.rtmp.youtube.com/live2/abcd-1234");
    assert_eq!((list[0].stream.as_str(), list[0].enabled), ("*", true));
    // A second one on the same platform gets an id of its own.
    assert_eq!(add(&mut list, &req).unwrap(), "youtube-2");
}

#[test]
fn the_platforms_the_page_added_are_taken_and_ask_for_what_they_hand_out() {
    let mut list = Vec::new();
    let kick = AddDestinationRequest { key: Some("sk_live".into()), ..add_req("kick") };
    assert_eq!(add(&mut list, &kick).unwrap(), "kick");
    assert!(list[0].url().starts_with("rtmps://fa723fc1b171.global-contribute.live-video.net:443/app/"));
    // Instagram hands out a server per stream, so a key alone is not enough.
    let err = add(&mut list, &AddDestinationRequest { key: Some("k".into()), ..add_req("instagram") }).unwrap_err();
    assert_eq!(field(&err), "server");
}

#[test]
fn a_label_becomes_the_id() {
    let mut list = Vec::new();
    let req = AddDestinationRequest {
        label: Some("Twitch, backup".into()),
        key: Some("live_1".into()),
        ..add_req("twitch")
    };
    assert_eq!(add(&mut list, &req).unwrap(), "twitch-backup");
}

#[test]
fn custom_and_srt_need_an_address_of_their_own_kind() {
    let mut list = Vec::new();
    assert_eq!(field(&add(&mut list, &add_req("custom")).unwrap_err()), "server");
    let srt_as_rtmp = AddDestinationRequest { server: Some("rtmp://h/live/k".into()), ..add_req("srt") };
    let err = add(&mut list, &srt_as_rtmp).unwrap_err();
    assert!(err.message.contains("srt://192.168.1.50:9000"), "{}", err.message);
    let srt = AddDestinationRequest {
        server: Some("srt://10.0.0.9:9000".into()),
        key: Some("ignored".into()),
        ..add_req("srt")
    };
    add(&mut list, &srt).unwrap();
    assert_eq!(list[0].key, None, "SRT keeps no key");
    // A whole address is enough for custom; a bare server is not.
    let bare = AddDestinationRequest { server: Some("rtmp://h/live".into()), ..add_req("custom") };
    assert_eq!(field(&add(&mut list, &bare).unwrap_err()), "key");
    let whole = AddDestinationRequest { server: Some("rtmp://h/live/stream".into()), ..add_req("custom") };
    add(&mut list, &whole).unwrap();
}

#[test]
fn an_unknown_platform_lists_the_ones_there_are() {
    let err = add(&mut Vec::new(), &add_req("myspace")).unwrap_err();
    assert_eq!(field(&err), "platform");
    assert_eq!(err.data["platforms"][0], "youtube");
}

#[test]
fn set_moves_only_what_it_names_and_keeps_the_key_it_was_not_given() {
    let mut list = Vec::new();
    add(&mut list, &AddDestinationRequest { key: Some("k1".into()), ..add_req("youtube") }).unwrap();
    let req = SetDestinationRequest {
        id: "sunday".into(),
        destination: "youtube".into(),
        enabled: Some(false),
        stream: Some("main_720p".into()),
        ..Default::default()
    };
    set(&mut list, &req).unwrap();
    assert_eq!(list[0].key.as_deref(), Some("k1"));
    assert!(!list[0].enabled);
    assert_eq!(list[0].stream, "main_720p");
    // Clearing the key YouTube cannot do without is refused, and the list is
    // left as it was.
    let clear = SetDestinationRequest { key: Some(String::new()), ..req.clone() };
    assert_eq!(field(&set(&mut list, &clear).unwrap_err()), "key");
    assert_eq!(list[0].key.as_deref(), Some("k1"));
    let missing = SetDestinationRequest { destination: "twitch".into(), ..req };
    let err = set(&mut list, &missing).unwrap_err();
    assert_eq!(err.data["valid"][0], "youtube");
    assert_eq!(err.data["channel"], "sunday");
}

#[test]
fn remove_takes_one_and_names_the_rest_when_it_misses() {
    let mut list = Vec::new();
    add(&mut list, &AddDestinationRequest { key: Some("k".into()), ..add_req("twitch") }).unwrap();
    assert!(remove(&mut list, "sunday", "youtube").is_err());
    assert_eq!(remove(&mut list, "sunday", "twitch").unwrap().id, "twitch");
    assert!(list.is_empty());
}

#[test]
fn a_recording_and_a_watch_link_need_no_address_and_no_key() {
    let mut list = Vec::new();
    assert_eq!(add(&mut list, &add_req("file")).unwrap(), "record");
    assert_eq!(add(&mut list, &add_req("hls")).unwrap(), "watch-link");
    assert!(list.iter().all(|d| d.server.is_empty() && d.key.is_none() && d.has_key()));
    // A key sent anyway is not kept: there is nobody to give it to.
    let keyed = AddDestinationRequest { key: Some("k".into()), ..add_req("file") };
    add(&mut list, &keyed).unwrap();
    assert_eq!(list[2].key, None);
    // A folder of its own, and params for the link.
    let there = AddDestinationRequest { server: Some("D:/Recordings".into()), ..add_req("file") };
    add(&mut list, &there).unwrap();
    let quick = AddDestinationRequest { server: Some("hls://?segment_ms=1000".into()), ..add_req("hls") };
    add(&mut list, &quick).unwrap();
}

#[test]
fn a_recording_or_a_watch_link_refuses_what_it_cannot_do_and_says_what_to_send() {
    let mut list = Vec::new();
    let elsewhere = AddDestinationRequest { server: Some("rtmp://host/live".into()), ..add_req("file") };
    let err = add(&mut list, &elsewhere).unwrap_err();
    assert_eq!(field(&err), "server");
    assert!(err.message.contains("a folder on this machine"), "{}", err.message);
    let err = add(&mut list, &AddDestinationRequest { server: Some("srt://h:9000".into()), ..add_req("hls") }).unwrap_err();
    assert!(err.message.contains("Leave `server` out"), "{}", err.message);
    let converted = AddDestinationRequest {
        rendition: Some(serde_json::from_value(serde_json::json!({"preset": "youtube-720p30"})).unwrap()),
        ..add_req("hls")
    };
    let err = add(&mut list, &converted).unwrap_err();
    assert_eq!(field(&err), "rendition");
    assert!(err.message.contains("converts nothing"), "{}", err.message);
    assert!(list.is_empty());
}
