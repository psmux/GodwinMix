//! Every sentence a person reads here names no program, setting, file or
//! plugin id; the detail does.

use super::web::Lookup;
use super::*;
use godwinmix_protocol::ActionKind;

/// Words that belong in a log and never in a sentence.
pub const INTERNAL: &[&str] = &[
    "godwinmix", "sidecar", "browser.", ".app", "plugin", "gst", "nicesrc", "webrtcbin", "rsvg",
    "audio-device", "ingest", "cargo", "CEF", "`",
];

pub fn plain(sentence: &str) {
    for word in INTERNAL {
        assert!(!sentence.contains(word), "`{word}` in a sentence for a person: {sentence}");
    }
    let dashes = ['\u{2014}', '\u{2013}', '\u{2015}'];
    assert!(!sentence.contains(dashes) && !sentence.contains("--"), "dash punctuation: {sentence}");
    assert!(sentence.ends_with('.'), "a sentence: {sentence}");
}

fn lookup() -> Lookup {
    Lookup {
        looked: vec!["/x/godwinmix-browser.app/Contents/MacOS/godwinmix-browser".into()],
        buildable: Some("/x/browser".into()),
        ..Default::default()
    }
}

#[test]
fn the_web_messages_are_plain_and_keep_the_detail() {
    let l = lookup();
    for a in [plain::web_setting_up(&l), plain::web_unavailable(&l), plain::web_configured_missing(&l)] {
        plain(&a.message);
        plain(&format!("{}.", a.action.label));
        let detail = a.detail.expect("a detail").to_string();
        assert!(detail.contains("godwinmix-browser") && detail.contains("browser.sidecar"), "{detail}");
    }
    let setting = plain::web_setting_up(&l);
    assert_eq!(setting.action.kind, ActionKind::Setup);
    assert_eq!(setting.action.piece.as_deref(), Some("web"));
    assert!(setting.message.contains("a few minutes"));
}

#[test]
fn the_plugin_messages_are_plain_and_name_the_plugin_only_in_the_detail() {
    for name in ["camera", "screen", "audio-device", "ingest", "ndi", "some-third-party"] {
        let shipped = name != "some-third-party";
        for a in [plain::plugin_missing(name, &format!("{name}/source"), shipped), plain::plugin_off(name)] {
            plain(&a.message);
            assert_eq!(a.detail.as_ref().unwrap()["plugin"], name);
            assert_eq!(a.action.name.as_deref().or(a.action.piece.as_deref()), Some(name));
        }
    }
    let cams = plain::plugin_missing("camera", "camera/source", true);
    assert!(cams.message.starts_with("Cameras are not set up"), "{}", cams.message);
    assert_eq!(cams.action.kind, ActionKind::Setup);
    assert_eq!(cams.action.label, "Set up cameras");
}

#[test]
fn a_missing_part_of_gstreamer_names_one_command_to_copy() {
    let a = system::missing("Sending the programme to a browser", system::NICE, &["nicesrc", "nicesink"]);
    plain(&a.message);
    assert_eq!(a.action.kind, ActionKind::Copy);
    let command = a.action.command.clone().unwrap();
    assert!(
        command.starts_with("brew install") || command.starts_with("sudo ") || command.starts_with("winget"),
        "{command}"
    );
    assert!(a.detail.unwrap()["elements"].to_string().contains("nicesrc"));
}

#[test]
fn setting_up_says_how_long() {
    plain(&plain::setting_up("web"));
    plain(&plain::setting_up("camera"));
    assert!(plain::setting_up("camera").contains("about a minute"));
    assert_eq!(names::title("audio-device"), "Microphones and audio");
    assert_eq!(names::noun("nothing-known"), "this feature");
}
