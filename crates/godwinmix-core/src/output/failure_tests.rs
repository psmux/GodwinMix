//! The messages here are the ones `rtmp2sink` posted on Windows with
//! GStreamer 1.28, copied off the bus, not made up.

use super::*;

const URI: &str = "rtmp://127.0.0.1:19351/live/abcd-efgh-secret";

#[test]
fn each_failure_gets_its_own_reason() {
    let cases = [
        ("Connection refused: Could not connect to 127.0.0.1: No connection could be made because the target machine actively refused it.", OutputErrorReason::Refused),
        ("Failed to connect: Could not connect to 192.0.2.1: Socket I/O timed out", OutputErrorReason::TimedOut),
        ("Failed to connect: Error resolving \u{201c}nonexistent.invalid\u{201d}: No such host is known.", OutputErrorReason::NotFound),
        ("Failed to connect: Short read (want 3073 have 0)", OutputErrorReason::Closed),
        ("Failed to connect: 'publish' cmd failed: NetStream.Publish.Denied: no such key", OutputErrorReason::Rejected),
        ("Failed to connect: Network unreachable", OutputErrorReason::Unreachable),
        (STALLED, OutputErrorReason::Stalled),
        ("Something nobody has seen", OutputErrorReason::Other),
    ];
    for (message, want) in cases {
        assert_eq!(classify(message, URI).reason, want, "{message}");
    }
}

/// A server that takes the connection and drops it at once. Windows reports
/// that as a reset (WSAECONNRESET) or, when the drop lands while the client
/// is still sending its handshake, as an abort (WSAECONNABORTED). These are
/// the operating system's own words for both, as a client on Windows 11 read
/// them; either way the far end hung up.
#[test]
fn a_hang_up_on_windows_reads_as_closed_however_windows_words_it() {
    for message in [
        "An existing connection was forcibly closed by the remote host.",
        "An established connection was aborted by the software in your host machine.",
    ] {
        assert_eq!(classify(message, URI).reason, OutputErrorReason::Closed, "{message}");
    }
}

#[test]
fn the_sentence_names_the_server_and_the_next_step() {
    let e = classify("Connection refused: Could not connect to 127.0.0.1", URI);
    assert!(e.message.starts_with("127.0.0.1:19351 refused"), "{}", e.message);
    assert!(e.message.contains("Check the server address"), "{}", e.message);
    let e = classify("Socket I/O timed out", "rtmp://a.rtmp.youtube.com/live2/abcd");
    assert!(e.message.contains("port 1935"), "the port a firewall would block: {}", e.message);
    let e = classify("Socket I/O timed out", "rtmps://live-api-s.facebook.com:443/rtmp/abcd");
    assert!(e.message.contains("port 443"), "{}", e.message);
    let e = classify("Error resolving: No such host is known", "rtmp://a.rtmp.yotube.com/live2/abcd");
    assert!(e.message.contains("no server called a.rtmp.yotube.com."), "{}", e.message);
    let e = classify("NetStream.Publish.Denied", URI);
    assert!(e.message.contains("stream key is wrong"), "{}", e.message);
}

#[test]
fn a_key_quoted_back_by_the_server_is_cut_out() {
    let e = classify("'publish' cmd failed: NetStream.Publish.Denied: abcd-efgh-secret is not a key", URI);
    assert!(!e.detail.contains("secret"), "{}", e.detail);
    assert!(!e.message.contains("secret"), "{}", e.message);
    let e = classify("Failed: live/abcd-efgh-secret?psk=hunter22", "rtmp://h/live/abcd-efgh-secret?psk=hunter22");
    assert!(!e.detail.contains("hunter22") && !e.detail.contains("secret"), "{}", e.detail);
}

#[test]
fn the_specific_error_wins_over_the_generic_ones_behind_it() {
    let f = Failure::default();
    f.attempt();
    f.note("Connection refused: Could not connect", URI);
    f.note("Internal data stream error.", URI);
    assert_eq!(f.current().unwrap().reason, OutputErrorReason::Refused);
    // The next attempt's first error replaces it, whatever it is.
    f.attempt();
    f.note("Socket I/O timed out", URI);
    assert_eq!(f.current().unwrap().reason, OutputErrorReason::TimedOut);
    // A generic error first is improved on by a specific one after it.
    f.attempt();
    f.note("Internal data stream error.", URI);
    f.note("Short read (want 3073 have 0)", URI);
    assert_eq!(f.current().unwrap().reason, OutputErrorReason::Closed);
}

#[test]
fn the_alert_goes_out_once_per_run_of_failures() {
    let f = Failure::default();
    assert!(f.alert("yt").is_none(), "nothing to say before an error");
    f.attempt();
    f.note("Connection refused", URI);
    let Some(Event::Alert { severity, message, action }) = f.alert("yt") else { panic!("no alert") };
    assert_eq!(severity, Severity::Error);
    assert!(message.starts_with("yt did not start."), "{message}");
    assert_eq!(action.unwrap().panel.as_deref(), Some("core/outputs"));
    assert!(f.alert("yt").is_none(), "told twice about one failure");
    // Live, then dropped: told again, as a warning this time.
    f.connected();
    assert!(f.current().is_none(), "a live output still carried its old error");
    f.attempt();
    f.note("Short read", URI);
    let Some(Event::Alert { severity, message, .. }) = f.alert("yt") else { panic!("no alert") };
    assert_eq!(severity, Severity::Warning);
    assert!(message.starts_with("yt lost its connection."), "{message}");
}
