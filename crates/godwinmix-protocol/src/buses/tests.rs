use super::*;

#[test]
fn a_bus_written_by_hand_reads_back_the_same() {
    let text = r#"
        [master]
        gain = 0.8

        [[bus]]
        id = "guest-return"
        minus = "guest-1"

        [[bus]]
        id = "monitor"
        sends = { cam1 = 0.5 }
    "#;
    let cfg: AudioConfig = toml::from_str(text).unwrap();
    assert_eq!(cfg.master.gain, 0.8);
    assert_eq!(cfg.buses[0].kind(), "minus");
    assert_eq!(cfg.buses[1].kind(), "sends");
    assert_eq!(cfg.buses[1].sends["cam1"], 0.5);
    let again: AudioConfig = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
    assert_eq!(cfg, again);
}
