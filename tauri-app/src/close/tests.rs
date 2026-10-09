use super::*;
use crate::running::Thing;

fn streaming() -> Known {
    Known::Things(vec![Thing { line: "Streaming to YouTube, live for 1:56:23".into(), outgoing: true, stop: None }])
}

#[test]
fn with_nothing_running_the_app_quits_mixer_and_all() {
    assert_eq!(decide(Some(&Known::Things(vec![])), true, false), Closing::Quit);
    assert_eq!(decide(Some(&Known::Things(vec![])), false, false), Closing::Quit);
    // The connect page, with no mixer at all.
    assert_eq!(decide(None, false, false), Closing::Quit);
}

#[test]
fn with_anything_running_it_asks_and_never_quits_or_hides_by_itself() {
    assert_eq!(decide(Some(&streaming()), true, false), Closing::AskLocal);
    assert_eq!(decide(Some(&streaming()), true, true), Closing::AskLocal);
    assert_eq!(decide(Some(&streaming()), false, false), Closing::AskRemote);
    assert_eq!(decide(Some(&streaming()), false, true), Closing::AskStop);
}

#[test]
fn a_mixer_that_does_not_answer_is_asked_about_rather_than_assumed_idle() {
    let silent = Known::Unknown("it did not answer in time".into());
    assert_eq!(decide(Some(&silent), true, false), Closing::AskLocal);
    assert!(question(&silent, &Closing::AskLocal, "").contains("may still be streaming"));
}

#[test]
fn each_button_means_what_it_says() {
    let custom = |s: &str| MessageDialogResult::Custom(s.to_string());
    assert_eq!(choice(&custom(STOP_AND_QUIT), true), Choice::StopAndQuit);
    assert_eq!(choice(&custom(BACKGROUND), true), Choice::Background);
    assert_eq!(choice(&custom(CANCEL), true), Choice::Cancel);
    assert_eq!(choice(&custom(LEAVE), false), Choice::Leave);
    // A platform that maps the three buttons to Yes, No and Cancel.
    assert_eq!(choice(&MessageDialogResult::Yes, true), Choice::StopAndQuit);
    assert_eq!(choice(&MessageDialogResult::No, true), Choice::Background);
    assert_eq!(choice(&MessageDialogResult::Cancel, true), Choice::Cancel);
    assert_eq!(choice(&MessageDialogResult::Ok, false), Choice::Leave);
    // Closing the dialog with its own X is Cancel, never a quit.
    assert_eq!(choice(&MessageDialogResult::default(), true), Choice::Cancel);
}

#[test]
fn the_question_names_what_is_running_and_what_each_answer_does() {
    let local = question(&streaming(), &Closing::AskLocal, "http://127.0.0.1:5000");
    assert!(local.contains("Streaming to YouTube, live for 1:56:23"), "{local}");
    assert!(local.contains("Stop everything and quit") && local.contains("Keep running in the background"), "{local}");
    let remote = question(&streaming(), &Closing::AskRemote, "http://studio.local:8080");
    assert!(remote.contains("studio.local") && remote.contains("leaves it running there"), "{remote}");
}
