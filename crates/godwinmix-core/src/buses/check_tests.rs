use super::*;

#[test]
fn a_bad_id_says_what_an_id_looks_like() {
    for id in ["", "Monitor", "a b", "-x", "x-", "a--b", "program"] {
        let e = bus_id(id).unwrap_err();
        assert!(matches!(e, Refusal::Invalid { field: "id", .. }), "{id:?} was taken");
    }
    bus_id("guest-return").unwrap();
}

#[test]
fn a_level_out_of_range_names_the_range() {
    let e = level("level", 11.0).unwrap_err().to_string();
    assert!(e.contains("0.0") && e.contains("10"), "{e}");
    assert!(level("gain", f64::NAN).is_err());
    assert_eq!(level("gain", 0.5).unwrap(), 0.5);
}

#[test]
fn a_bus_that_is_cue_and_minus_is_refused() {
    let mut cfg = BusConfig::named("hp");
    cfg.cue = true;
    cfg.minus = Some("guest-1".into());
    assert!(bus(&cfg).unwrap_err().to_string().contains("Drop"));
}
