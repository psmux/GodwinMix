use super::*;

fn programme(source: &str, pad_volume: f64, muted: bool) -> Programme {
    gst::init().unwrap();
    let amute = gst::ElementFactory::make("volume").build().unwrap();
    amute.set_property("mute", muted);
    let mix = gst::ElementFactory::make("audiomixer").build().unwrap();
    let apad = mix.request_pad_simple("sink_%u").unwrap();
    apad.set_property("volume", pad_volume);
    Programme { source: source.into(), amute, apad }
}

fn weights(mode: Mode, sends: &[(&str, f64)]) -> Weights {
    Weights { mode, sends: sends.iter().map(|(k, v)| (k.to_string(), *v)).collect() }
}

#[test]
fn a_send_is_its_level_and_nothing_while_muted() {
    let w = weights(Mode::Sends, &[("cam1", 0.5)]);
    assert_eq!(target(&w, &[], &programme("cam1", 0.0, false)), 0.5);
    assert_eq!(target(&w, &[], &programme("cam1", 1.0, true)), 0.0);
    assert_eq!(target(&w, &[], &programme("cam2", 1.0, false)), 0.0);
}

#[test]
fn a_mix_minus_follows_the_programme_and_leaves_its_source_out() {
    let w = weights(Mode::Minus("guest-1".into()), &[]);
    assert_eq!(target(&w, &[], &programme("host", 0.7, false)), 0.7);
    assert_eq!(target(&w, &[], &programme("guest-1", 1.0, false)), 0.0);
}

#[test]
fn cue_hears_what_is_marked_even_muted_and_the_programme_otherwise() {
    let w = weights(Mode::Cue, &[]);
    let marked = vec!["mic".to_string()];
    assert_eq!(target(&w, &marked, &programme("mic", 0.0, true)), 1.0);
    assert_eq!(target(&w, &marked, &programme("cam1", 1.0, false)), 0.0);
    assert_eq!(target(&w, &[], &programme("cam1", 1.0, false)), 1.0);
}

#[test]
fn a_weight_moves_in_steps() {
    assert_eq!(step(0.0, 1.0), STEP);
    assert_eq!(step(1.0, 0.0), 1.0 - STEP);
    assert_eq!(step(0.5, 0.55), 0.55);
}
