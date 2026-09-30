use crate::testing::{gpu, profile, shape, x264};
use godwinmix_protocol::rendition::{AudioCodec, AudioShape, VideoCodec};

#[test]
fn hardware_comes_first_among_the_encoders_for_a_codec() {
    let p = profile();
    assert_eq!(p.encoders(VideoCodec::H264), vec![gpu(), x264()]);
    assert!(p.encoders(VideoCodec::Av1).is_empty());
}

#[test]
fn an_encode_costs_what_was_measured_at_a_measured_shape() {
    let p = profile();
    let c = p.encode_cost(&x264(), &shape(1920, 1080, 30));
    assert!((1990..=2010).contains(&c.cpu_millicores), "{c:?}");
    assert_eq!(c.device_sessions, 0);
    let c = p.encode_cost(&x264(), &shape(1920, 1080, 60));
    assert!((3990..=4010).contains(&c.cpu_millicores), "twice the pixels, twice the cost: {c:?}");
}

#[test]
fn a_hardware_encode_holds_a_session_and_a_share_of_the_device() {
    let c = profile().encode_cost(&gpu(), &shape(1920, 1080, 30));
    assert_eq!(c.device_sessions, 1);
    assert!((195..=205).contains(&c.device_millis), "{c:?}");
    assert!(c.cpu_millicores < 100);
}

#[test]
fn presets_run_fastest_first_and_the_best_that_fits_is_chosen() {
    let p = profile();
    assert_eq!(p.presets(&x264()), ["ultrafast", "superfast", "veryfast", "faster"]);
    let s = shape(1920, 1080, 30);
    assert_eq!(p.preset_that_fits(&x264(), &s, 1500).as_deref(), Some("superfast"));
    assert_eq!(p.preset_that_fits(&x264(), &s, 5000).as_deref(), Some("faster"));
    assert_eq!(p.preset_that_fits(&x264(), &s, 500), None);
    assert_eq!(p.faster_preset(&x264(), None).as_deref(), Some("superfast"));
    assert_eq!(p.faster_preset(&x264(), Some("ultrafast")), None);
}

#[test]
fn the_session_limit_is_the_one_the_device_refused_at() {
    let p = profile();
    assert_eq!(p.session_limit("gpu"), Some(3));
    assert_eq!(p.session_limit("other"), None);
}

#[test]
fn an_uncalibrated_machine_still_prices_work_cautiously() {
    let p = crate::Profile::uncalibrated();
    assert!(!p.is_calibrated());
    let c = p.encode_cost(&x264(), &shape(1920, 1080, 30));
    assert!(c.cpu_millicores > 1000, "{c:?}");
    assert!(p.decode_cost(&shape(1920, 1080, 30)).cpu_millicores > 100);
    let a = p.audio_cost(&AudioShape { codec: AudioCodec::Aac, channels: 2, sample_rate: 48_000, bitrate_kbps: 160 });
    assert!(a.cpu_millicores > 0 && a.cpu_millicores < 100);
}

#[test]
fn scaling_costs_by_the_pixels_in_and_out() {
    let c = profile().scale_cost(&shape(1920, 1080, 30), &shape(1280, 720, 30));
    // 2.0 a megapixel second, over 62.2 in and 27.6 out.
    assert!((175..=185).contains(&c.cpu_millicores), "{c:?}");
}
