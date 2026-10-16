use super::*;

fn params(s: &str) -> Params {
    toml::from_str(s).unwrap()
}

#[test]
fn the_gpu_is_off_unless_a_page_asks_for_it() {
    assert!(!wants_gpu(&params("")));
    assert!(!wants_gpu(&params("gpu = false")));
    assert!(wants_gpu(&params("gpu = true")));
}

#[test]
fn gpu_is_true_or_false_and_the_refusal_names_the_field() {
    assert!(validate(&params("gpu = true")).is_ok());
    let err = validate(&params("gpu = \"yes\"")).unwrap_err().to_string();
    assert!(err.contains("params.gpu"), "{err}");
}

/// The schema the kind table publishes is what the UI's settings form and a
/// third party client draw the switch from.
#[test]
fn the_kind_table_offers_the_switch_off_by_default() {
    let schema = super::super::schema::params("browser/source").expect("browser/source publishes its params");
    assert_eq!(schema["properties"]["gpu"]["type"], "boolean");
    assert_eq!(schema["properties"]["gpu"]["default"], false);
}
