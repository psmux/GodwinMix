use super::*;

fn rule(backup: &str) -> OnLoss {
    OnLoss::to(backup)
}

#[test]
fn the_short_form_in_a_config_takes_every_default() {
    let r: OnLoss = toml::from_str("backup = \"cam-wide\"").unwrap();
    assert_eq!(r, OnLoss::to("cam-wide"));
    assert_eq!(r.returns, ReturnMode::Auto);
    assert_eq!(r.after_secs, DEFAULT_AFTER_SECS);
}

/// TOML writes `after_secs = 3` as an integer, and it has to read as seconds.
#[test]
fn whole_seconds_in_toml_read_as_seconds() {
    let r: OnLoss = toml::from_str("backup = \"b\"\nafter_secs = 0\nreturn_after_secs = 8").unwrap();
    assert_eq!((r.after_secs, r.return_after_secs), (0.0, 8.0));
}

#[test]
fn the_full_form_reads_as_written() {
    let r: OnLoss = serde_json::from_value(serde_json::json!({
        "backup": "cam-wide", "after_secs": 0, "transition": "dissolve",
        "duration_ms": 500, "return": "manual", "return_after_secs": 2
    }))
    .unwrap();
    assert_eq!(r.after_secs, 0.0);
    assert_eq!(r.transition_name(), "fade", "dissolve is a fade here");
    assert_eq!(r.duration_ms, Some(500));
    assert_eq!(r.returns, ReturnMode::Manual);
    let back = serde_json::to_value(&r).unwrap();
    assert_eq!(back["return"], "manual", "written under the name it is read by");
}

#[test]
fn a_misspelt_key_is_refused_rather_than_ignored() {
    let e = serde_json::from_value::<OnLoss>(serde_json::json!({ "backup": "b", "after": 3 }));
    assert!(e.is_err(), "`after` was quietly dropped");
}

#[test]
fn an_unknown_backup_is_refused_with_the_ids_there_are() {
    let r = rule("cam-wdie");
    let e = check(&[("cam-main", &r)], &["cam-main", "cam-wide", "clip"]).unwrap_err();
    assert!(e.message.contains("no source called cam-wdie"), "{}", e.message);
    assert!(e.message.contains("cam-wide, clip"), "{}", e.message);
    assert_eq!(e.data["sources"], serde_json::json!(["cam-wide", "clip"]));
}

#[test]
fn a_source_cannot_back_itself_up() {
    let r = rule("cam-main");
    let e = check(&[("cam-main", &r)], &["cam-main"]).unwrap_err();
    assert!(e.message.contains("its own backup"), "{}", e.message);
}

#[test]
fn a_chain_is_allowed_and_a_loop_is_not() {
    let (a, b, c) = (rule("b"), rule("c"), rule("a"));
    let ids = ["a", "b", "c"];
    assert!(check(&[("a", &a), ("b", &b)], &ids).is_ok(), "a chain of two is fine");
    let e = check(&[("a", &a), ("b", &b), ("c", &c)], &ids).unwrap_err();
    assert!(e.message.contains("c -> a -> b -> c"), "{}", e.message);
    assert_eq!(e.data["cycle"], serde_json::json!(["c", "a", "b", "c"]));
}

#[test]
fn numbers_out_of_range_name_the_field() {
    let mut r = rule("b");
    r.after_secs = -1.0;
    let e = check(&[("a", &r)], &["a", "b"]).unwrap_err();
    assert_eq!(e.data["field"], "after_secs");
    r.after_secs = 3.0;
    r.duration_ms = Some(60_000);
    assert_eq!(check(&[("a", &r)], &["a", "b"]).unwrap_err().data["field"], "duration_ms");
}

#[test]
fn a_set_lays_only_what_it_names_over_the_rule() {
    let mut current = rule("cam-wide");
    current.after_secs = 7.0;
    let req = SetOnLossRequest { id: "cam-main".into(), returns: Some(ReturnMode::Manual), ..Default::default() };
    let r = req.over(Some(&current)).unwrap();
    assert_eq!((r.backup.as_str(), r.after_secs, r.returns), ("cam-wide", 7.0, ReturnMode::Manual));
    let first = SetOnLossRequest { id: "cam-main".into(), ..Default::default() };
    assert!(first.over(None).unwrap_err().contains("\"backup\": \"cam-wide\""));
}

#[test]
fn every_built_in_transition_and_dissolve_are_known() {
    assert!(known_transition("Dissolve"));
    assert!(known_transition("wipe"));
    assert!(!known_transition("spin"));
}
