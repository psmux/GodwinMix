use super::*;
use serde_json::json;

const MF: &str = r"\\?\usb#vid_3277&pid_0055&mi_00#6&21fe1b1b&0&0000#{e5323777-f976-4f5b-9b55-b94699c46e44}\global";
const KS: &str = r"\\?\usb#vid_3277&pid_0055&mi_00#6&21fe1b1b&0&0000#{6994ad05-93ef-11d0-a3cc-00a0c9223196}\global";

#[test]
fn one_camera_under_both_windows_providers_is_the_same_camera() {
    assert!(same_camera(MF, KS));
    assert!(same_camera(&MF.to_uppercase(), KS));
    let other = KS.replace("21fe1b1b", "11111111");
    assert!(
        !same_camera(MF, &other),
        "two cameras of one model are two cameras"
    );
    assert!(!same_camera("", KS));
    assert!(!same_camera(MF, ""));
}

#[test]
fn this_platform_has_a_capture_element_named_for_it() {
    assert!(!CANDIDATES.is_empty());
    if cfg!(target_os = "macos") {
        assert_eq!(CANDIDATES, &["avfvideosrc"]);
    }
    if cfg!(target_os = "windows") {
        assert_eq!(
            CANDIDATES,
            &["ksvideosrc", "mfvideosrc"],
            "Kernel Streaming first, and the Media Foundation fallback must stay"
        );
    }
    if cfg!(target_os = "linux") {
        assert_eq!(CANDIDATES, &["v4l2src"]);
    }
}

#[test]
fn a_forced_element_that_does_not_exist_names_itself() {
    godwinmix_capture_common::init().unwrap();
    let settings = Settings::from(&json!({"element": "v4l2src"}));
    if elements::exists("v4l2src") {
        return; // on Linux this is the real path and is tested elsewhere
    }
    let err = choose(&settings, Route::Fast)
        .err()
        .expect("not on this platform");
    assert!(err.contains("v4l2src"), "{err}");
}

#[test]
fn a_forced_element_has_no_device_modes_to_offer() {
    godwinmix_capture_common::init().unwrap();
    let settings = Settings::from(&json!({"element": "videotestsrc"}));
    let Ok(chosen) = choose(&settings, Route::Fast) else {
        panic!("a test pattern exists everywhere");
    };
    assert_eq!(chosen.via, "videotestsrc");
    assert!(chosen.caps.is_none() && chosen.sizes.is_empty());
}
