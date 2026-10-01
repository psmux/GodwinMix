use super::*;
use serde_json::json;

#[test]
fn from_reads_a_name_a_show_or_a_project() {
    let named: ShowFrom = serde_json::from_value(json!("empty")).unwrap();
    assert_eq!(named, ShowFrom::Named("empty".into()));
    let project: ShowFrom = serde_json::from_value(json!({"project": {"name": "x"}})).unwrap();
    assert!(matches!(project, ShowFrom::Project { .. }));
}

#[test]
fn a_show_says_its_state_in_lowercase() {
    let show = Show {
        id: "main".into(),
        name: "Main".into(),
        state: ShowState::Running,
        on_air: None,
        programme_kbps: 0,
        cpu_millicores: 0,
        memory_mib: 0,
        restarts: 0,
        error: None,
        compositing: true,
        input: None,
        outputs: vec![],
        health: Health::default(),
        alarms: None,
    };
    assert_eq!(serde_json::to_value(&show).unwrap()["state"], "running");
}

#[test]
fn a_show_written_before_wave_four_composites() {
    let show: Show = serde_json::from_value(json!({
        "id": "main", "name": "Main", "state": "running", "on_air": null,
        "programme_kbps": 0, "cpu_millicores": 0
    }))
    .unwrap();
    assert!(show.compositing);
    assert!(show.input.is_none() && show.outputs.is_empty());
}
