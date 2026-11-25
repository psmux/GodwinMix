use super::*;

const RATE: u32 = 48000;

/// `secs` of a stereo sine at `level` of full scale, at 1 kHz.
fn tone(level: f32, secs: f32) -> Vec<f32> {
    let n = (RATE as f32 * secs) as usize;
    (0..n).flat_map(|i| {
        let s = level * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / RATE as f32).sin();
        [s, s]
    }).collect()
}

fn meter() -> Loudness {
    let mut l = Loudness::default();
    l.prepare(RATE, 2);
    l
}

/// Run `samples` through in 20 ms buffers, as the pipeline would.
fn run(l: &mut Loudness, samples: &mut [f32]) {
    for chunk in samples.chunks_mut(960 * 2) {
        l.process(chunk, 2);
    }
}

#[test]
fn a_full_scale_sine_reads_where_bs_1770_says() {
    // EBU Tech 3341: a 1 kHz sine at 0 dBFS in both channels reads 0 LUFS,
    // to within a tenth of a decibel.
    let mut l = meter();
    l.set(&toml::from_str("max_cut_db = 0\nmax_boost_db = 0").unwrap()).unwrap();
    let mut s = tone(1.0, 4.0);
    run(&mut l, &mut s);
    let lufs = l.short_term().unwrap();
    assert!(lufs.abs() < 0.1, "{lufs}");
}

#[test]
fn a_quiet_voice_is_brought_up_to_the_target() {
    // A -30 dBFS sine reads -30 LUFS, 16 under -14, so the 12 dB boost
    // ceiling holds it at about -18 once it has had time to get there.
    let mut l = meter();
    l.set(&toml::from_str("speed_db_per_s = 10").unwrap()).unwrap();
    let mut s = tone(db_to_gain(-30.0), 8.0);
    run(&mut l, &mut s);
    assert!((l.gain_db - 12.0).abs() < 0.1, "{}", l.gain_db);
}

#[test]
fn the_gain_moves_no_faster_than_it_is_told() {
    let mut l = meter();
    l.set(&toml::from_str("speed_db_per_s = 1").unwrap()).unwrap();
    let mut s = tone(db_to_gain(-40.0), 2.0);
    run(&mut l, &mut s);
    assert!(l.gain_db <= 2.05, "{}", l.gain_db);
}

#[test]
fn silence_holds_the_gain_where_it_was() {
    let mut l = meter();
    l.gain_db = 3.0;
    let mut s = vec![0.0f32; RATE as usize * 4];
    run(&mut l, &mut s);
    assert_eq!(l.gain_db, 3.0);
}

#[test]
fn a_target_out_of_range_names_the_field() {
    let e = Loudness::default().set(&toml::from_str("target_lufs = 0").unwrap()).unwrap_err().to_string();
    assert!(e.contains("loudness/filter params.target_lufs"), "{e}");
}
