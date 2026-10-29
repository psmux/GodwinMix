use super::*;

fn tone(level: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| level * (i as f32 * 0.13).sin()).collect()
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

fn run(mode: Mode, params: &str, input: &mut [f32]) {
    let mut d = Dynamics::new(mode);
    d.prepare(48000, 1);
    d.set(&toml::from_str(params).unwrap()).unwrap();
    d.process(input, 1);
}

#[test]
fn nothing_leaves_the_limiter_above_its_ceiling() {
    let mut s = tone(1.0, 48000);
    run(Mode::Limit, "threshold_db = -6", &mut s);
    assert!(peak(&s) <= db_to_gain(-6.0) + 1e-6, "{}", peak(&s));
}

#[test]
fn a_loud_voice_is_brought_down_by_the_ratio() {
    // 0 dBFS into -20 dB at 4:1 with no knee: 20 dB over comes out 5 over.
    let mut s = tone(1.0, 48000);
    run(Mode::Compress, "threshold_db = -20\nratio = 4\nknee_db = 0\nattack_ms = 1", &mut s);
    let db = 20.0 * peak(&s[24000..]).log10();
    assert!((db + 15.0).abs() < 1.0, "{db} dB");
}

#[test]
fn a_quiet_voice_under_the_threshold_is_left_alone() {
    let mut s = tone(0.01, 4800);
    let before = peak(&s);
    run(Mode::Compress, "threshold_db = -10", &mut s);
    assert!((peak(&s) - before).abs() < 1e-4);
}

#[test]
fn an_expander_pushes_the_room_further_down() {
    let mut s = tone(0.003, 48000);
    run(Mode::Expand, "threshold_db = -30\nratio = 3", &mut s);
    assert!(peak(&s[24000..]) < 0.003 * 0.1, "{}", peak(&s[24000..]));
}

#[test]
fn obs_names_are_read() {
    let mut d = Dynamics::new(Mode::Compress);
    d.set(&toml::from_str("threshold = -30.0\nratio = 8.0\nattack_time = 2\nrelease_time = 100\noutput_gain = 3.0").unwrap())
        .unwrap();
    assert_eq!(d.s.threshold_db, -30.0);
    assert_eq!(d.s.ratio, 8.0);
    assert_eq!(d.s.output_db, 3.0);
}

#[test]
fn a_ratio_out_of_range_is_refused_and_the_old_settings_stay() {
    let mut d = Dynamics::new(Mode::Compress);
    let e = d.set(&toml::from_str("ratio = 500").unwrap()).unwrap_err().to_string();
    assert!(e.contains("compressor/filter params.ratio"), "{e}");
    assert_eq!(d.s.ratio, 4.0);
}
