use super::*;

fn params(text: &str) -> Params {
    toml::from_str(text).unwrap()
}

#[test]
fn nothing_set_is_a_working_cutout() {
    let s = Settings::from_params(&Params::new()).unwrap();
    assert_eq!(s.quality, Quality::Auto);
    let c = s.curve();
    assert_eq!((c[0], c[255]), (0, 255));
    assert_eq!((s.cutoff, s.softness, s.steady, s.feather), (0.6, 0.2, 0.5, 0));
    assert_eq!(c[128], 0);
    assert!(c[153] > 120 && c[153] < 136);
}

#[test]
fn a_wrong_setting_names_what_it_takes() {
    let err = Settings::from_params(&params("blur = 3")).unwrap_err().to_string();
    assert!(err.contains("quality") && err.contains("softness"), "{err}");
    let err = Settings::from_params(&params("steady = 2.0")).unwrap_err().to_string();
    assert!(err.contains("0 to 0.95"), "{err}");
    let err = Settings::from_params(&params("matte_left = 0.5\nmatte_right = 0.5")).unwrap_err().to_string();
    assert!(err.contains("leaves nothing"), "{err}");
}

#[test]
fn a_model_of_ones_own_is_named_by_its_file() {
    let s = Settings::from_params(&params("model = \"rvm_mobilenetv3\"")).unwrap();
    assert_eq!(s.quality, Quality::Named("rvm_mobilenetv3".into()));
    assert!(Settings::from_params(&params("model = \"../escape\"")).is_err());
}

#[test]
fn a_hard_edge_is_a_step_at_the_cutoff() {
    let s = Settings { cutoff: 0.5, softness: 0.0, ..Settings::default() };
    let c = s.curve();
    assert_eq!((c[126], c[129]), (0, 255));
}
