use super::*;

#[test]
fn an_empty_replay_table_is_off_and_writes_nothing() {
    let cfg: ReplayConfig = serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(!cfg.is_on());
    assert!(cfg.is_default());
    assert_eq!(cfg.seconds, DEFAULT_SECONDS);
    assert_eq!(serde_json::to_value(&cfg).unwrap(), serde_json::json!({}));
}

#[test]
fn a_replay_table_reads_back_as_written() {
    let cfg: ReplayConfig =
        serde_json::from_value(serde_json::json!({"seconds": 120, "angles": ["programme", "cam1"]})).unwrap();
    assert!(cfg.is_on());
    assert_eq!(cfg.angles, vec!["programme", "cam1"]);
    let again: ReplayConfig = serde_json::from_value(serde_json::to_value(&cfg).unwrap()).unwrap();
    assert_eq!(again, cfg);
}

#[test]
fn a_mark_point_is_a_lowercase_word() {
    let m: MarkRequest = serde_json::from_value(serde_json::json!({"point": "in", "seconds": 3})).unwrap();
    assert_eq!(m.point, Some(MarkPoint::In));
    assert!(serde_json::from_value::<MarkRequest>(serde_json::json!({"point": "start"})).is_err());
}
