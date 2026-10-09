use super::*;

#[test]
fn a_kept_output_is_stopped_and_listed_once() {
    let mut list = HeldList::default();
    let cfg = OutputConfig::bare("youtube", "rtmp://a.rtmp.youtube.com/live2/abcd-efgh");
    list.put(&cfg);
    list.put(&cfg);
    assert_eq!(list.configs().count(), 1);
    assert!(!list.configs().next().unwrap().enabled);
    let status: Vec<_> = list.statuses().collect();
    assert_eq!(status[0].state, OutputState::Stopped);
    assert!(status[0].has_key);
    assert!(!status[0].uri_host.contains("abcd"), "the key never leaves");
    assert_eq!(list.take("youtube").map(|c| c.id), Some("youtube".to_string()));
    assert!(!list.has("youtube"));
}
