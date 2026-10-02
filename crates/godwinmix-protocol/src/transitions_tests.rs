//! The refusals a client sees, checked word for word where it matters.

use super::*;

fn params(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

#[test]
fn a_direction_nobody_has_lists_the_ones_there_are() {
    let e = check_params("wipe", &params(json!({"direction": "sideways"}))).unwrap_err();
    assert!(e.message.contains("left, right, up, down"), "{}", e.message);
    assert_eq!(e.data["directions"], json!(DIRECTIONS));
    assert!(check_params("slide", &params(json!({"direction": "UP"}))).is_ok());
}

#[test]
fn an_easing_is_checked_on_every_transition() {
    let e = check_params("fade", &params(json!({"easing": "bounce"}))).unwrap_err();
    assert_eq!(e.data["easings"], json!(EASINGS));
    assert!(check_params("zoom", &params(json!({"easing": "ease-out"}))).is_ok());
}

#[test]
fn a_dip_takes_a_name_or_a_hex_colour() {
    assert_eq!(parse_colour("white"), Some(0xffff_ffff));
    assert_eq!(parse_colour("#1F6F4F"), Some(0xff1f_6f4f));
    assert_eq!(parse_colour("teal"), None);
    assert!(check_params("dip", &params(json!({"colour": "teal"}))).is_err());
    assert!(check_params("dip", &params(json!({"color": "#000000"}))).is_ok());
}

#[test]
fn a_zoom_point_is_a_fraction_of_the_canvas() {
    assert!(check_params("zoom", &params(json!({"x": 0.2, "y": 1.0}))).is_ok());
    let e = check_params("box", &params(json!({"x": 1.5}))).unwrap_err();
    assert!(e.message.contains("0 to 1"), "{}", e.message);
}

#[test]
fn an_item_transition_names_its_choices_when_it_is_wrong() {
    assert!(check_item("enter", &json!({"type": "slide", "edge": "left"})).is_ok());
    assert!(check_item("exit", &Value::Null).is_ok());
    let e = check_item("enter", &json!({"type": "spin"})).unwrap_err();
    assert_eq!(e.data["transitions"], json!(ITEM_TRANSITIONS));
    let e = check_item("exit", &json!({"type": "slide", "edge": "middle"})).unwrap_err();
    assert_eq!(e.data["edges"], json!(EDGES));
}
