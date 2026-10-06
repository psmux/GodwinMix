use super::*;

fn output(id: &str) -> OutputConfig {
    toml::from_str(&format!("id = \"{id}\"\nuri = \"rtmp://example.com/live/key\"\n")).expect("an output")
}

fn refused() -> anyhow::Error {
    let r = crate::render::Refusal { code: ErrorCode::Safety, message: "0.0 cores is free".into(), data: serde_json::json!({}) };
    anyhow::Error::new(r).context("attaching output archive")
}

#[test]
fn a_refused_output_is_kept_reported_and_due_again_soon() {
    let stall: StallConfig = toml::from_str("").expect("the defaults");
    let mut list = UnattachedList::default();
    let t0 = Instant::now();
    list.note(&output("archive"), &refused(), &stall, t0);
    assert!(list.has("archive"));
    assert!(list.due(t0).is_empty(), "not at once");
    assert_eq!(list.due(t0 + Duration::from_millis(500)).len(), 1, "after half a second");
    let status: Vec<_> = list.statuses().collect();
    assert_eq!(status[0].state, OutputState::Failed);
    assert!(status[0].shed.as_deref().unwrap_or("").contains("0.0 cores is free"), "{:?}", status[0].shed);

    // A second refusal waits longer, and the list still has one entry.
    list.note(&output("archive"), &refused(), &stall, t0);
    assert_eq!(list.configs().count(), 1);
    assert!(list.due(t0 + Duration::from_millis(500)).is_empty());
    assert!(list.forget("archive") && !list.has("archive"));
}

#[test]
fn a_governor_refusal_is_asked_again_within_ten_seconds_however_often() {
    let stall: StallConfig = toml::from_str("").expect("the defaults");
    assert!(delay(&stall, 40, true) <= Duration::from_secs(10));
    assert!(delay(&stall, 40, false) > Duration::from_secs(10), "anything else backs off further");
}
