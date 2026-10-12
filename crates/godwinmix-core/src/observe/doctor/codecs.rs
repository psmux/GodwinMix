//! The doctor's codec lines: which AAC encoder the programme gets, and what
//! this machine takes from gst-libav.
//!
//! Both read the codec catalogue, so they say what selection will do rather
//! than what a list in this file guesses it will.

use super::{Check, Verdict};
use crate::catalogue::libav::{in_use, is_libav, ladders, undecodable};
use crate::catalogue::model::Role;
use crate::catalogue::select::{GstRegistry, Registry};
use crate::catalogue::Catalogue;

/// The AAC encoder the programme will use, from the catalogue.
pub fn audio_encoder(cat: &Catalogue, reg: &dyn Registry) -> Check {
    let ladder = ladders(cat, Role::Encode, reg).remove("aac").unwrap_or_default();
    let names: Vec<&str> = ladder.iter().map(|(e, _)| e.as_str()).collect();
    match names.split_first() {
        None => Check::new(
            "audio encoder",
            Verdict::Fail,
            "no AAC encoder is installed, so nothing can be streamed. Install gst-plugins-bad for voaacenc, \
             gstreamer1.0-fdkaac for fdkaacenc, or gst-libav for avenc_aac",
        ),
        Some((first, _)) if is_libav(first) => Check::new(
            "audio encoder",
            Verdict::Warn,
            format!(
                "{first}, from gst-libav, which is the last resort. Install gst-plugins-bad for voaacenc, \
                 or gstreamer1.0-fdkaac for fdkaacenc, and that is used instead"
            ),
        ),
        Some((first, [])) => Check::new("audio encoder", Verdict::Ok, format!("{first} will be chosen")),
        Some((first, rest)) => Check::new(
            "audio encoder",
            Verdict::Ok,
            format!("{first} will be chosen; also here: {}", rest.join(", ")),
        ),
    }
}

/// Whether gst-libav is installed, what it is used for, and what nothing
/// here can decode.
pub fn libav(cat: &Catalogue, reg: &dyn Registry, installed: bool) -> Check {
    let used: Vec<String> =
        in_use(cat, reg).into_iter().map(|(role, codec, element)| format!("{codec} {role} ({element})")).collect();
    let missing = undecodable(cat, reg);
    let gap = if missing.is_empty() {
        String::new()
    } else {
        format!(" Nothing here decodes {}, so a source in it will not open.", missing.join(", "))
    };
    let (verdict, line) = match (installed, used.is_empty()) {
        (true, true) => (Verdict::Ok, "installed, and only a fallback: every codec has an element outside it.".to_string()),
        (true, false) => (Verdict::Ok, format!("installed, and used for {}. Nothing else here handles those.", used.join(", "))),
        (false, _) if missing.is_empty() => (Verdict::Ok, "not installed, and nothing needs it.".to_string()),
        (false, _) => (Verdict::Warn, "not installed.".to_string()),
    };
    Check::new("gst-libav", verdict, format!("{line}{gap}"))
}

/// Both lines, against this process's registry.
pub fn checks() -> Vec<Check> {
    let _ = gstreamer::init();
    let cat = crate::catalogue::global();
    let installed = gstreamer::Registry::get().find_plugin("libav").is_some();
    vec![audio_encoder(&cat, &GstRegistry), libav(&cat, &GstRegistry, installed)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::select::FakeRegistry;

    #[test]
    fn the_best_aac_encoder_is_named_and_libav_alone_is_a_warning() {
        let cat = Catalogue::shipped().unwrap();
        let vo = audio_encoder(&cat, &FakeRegistry::with(&["voaacenc", "avenc_aac"]));
        assert_eq!(vo.verdict, Verdict::Ok);
        assert!(vo.detail.starts_with("voaacenc will be chosen; also here: avenc_aac"), "{}", vo.detail);
        let av = audio_encoder(&cat, &FakeRegistry::with(&["avenc_aac"]));
        assert_eq!(av.verdict, Verdict::Warn, "{}", av.detail);
        let none = audio_encoder(&cat, &FakeRegistry::with(&[]));
        assert_eq!(none.verdict, Verdict::Fail);
    }

    #[test]
    fn the_libav_line_says_what_it_is_used_for_and_what_cannot_be_decoded() {
        let cat = Catalogue::shipped().unwrap();
        let with = FakeRegistry::with(&["openh264dec", "avdec_h265", "voaacenc", "faad"]);
        let line = libav(&cat, &with, true);
        assert!(line.detail.contains("h265 decode (avdec_h265)"), "{}", line.detail);
        let without = FakeRegistry::with(&["openh264dec", "voaacenc", "faad"]);
        let line = libav(&cat, &without, false);
        assert_eq!(line.verdict, Verdict::Warn);
        assert!(line.detail.contains("h265"), "{}", line.detail);
    }
}
