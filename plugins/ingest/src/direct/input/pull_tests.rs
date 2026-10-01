use super::*;
use crate::direct::input::rtsp::Rtsp;

#[test]
fn a_camera_password_never_reaches_a_message() {
    assert_eq!(redact("rtsp://admin:hunter2@10.0.0.9/stream1"), "rtsp://admin:***@10.0.0.9/stream1");
    assert_eq!(redact("rtsp://10.0.0.9/stream1"), "rtsp://10.0.0.9/stream1");
    let srt = crate::direct::input::ts_in::Srt::new(&InputSpec::new("srt://10.0.0.9:9000?passphrase=hunter2&latency=200"));
    assert_eq!(crate::direct::input::runner::Plan::address(&srt), "srt://10.0.0.9:9000?passphrase=***&latency=200");
}

#[test]
fn a_missing_file_and_a_bad_transport_say_what_to_do() {
    let err = File::new(&InputSpec::new("file:///no/such/clip.ts")).err().unwrap();
    assert!(err.message.contains("/no/such/clip.ts"), "{err}");
    let spec = InputSpec { params: json!({"transport": "quic"}), ..InputSpec::new("rtsp://cam/s") };
    assert_eq!(Rtsp::new(&spec).err().unwrap().data["field"], "params.transport");
}
