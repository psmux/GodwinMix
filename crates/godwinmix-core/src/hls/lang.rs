//! What a sound track is called in a player's menu, from the language its
//! PMT gave it.
//!
//! A PMT carries ISO 639-2 codes, three letters (`eng`, `spa`, and the
//! bibliographic `fre` or `ger` beside `fra` and `deu`). HLS wants RFC 5646
//! in `LANGUAGE`, which takes the two letter code where there is one, so
//! the known codes are turned into those, and anything else that is three
//! letters goes as it came. `NAME` is the language in English when it is
//! known, else the code, else `Track N`.

/// ISO 639-2 codes (terminology, then bibliographic), ISO 639-1, English name.
const KNOWN: &[(&str, &str, &str, &str)] = &[
    ("eng", "eng", "en", "English"),
    ("spa", "spa", "es", "Spanish"),
    ("fra", "fre", "fr", "French"),
    ("por", "por", "pt", "Portuguese"),
    ("deu", "ger", "de", "German"),
    ("ita", "ita", "it", "Italian"),
    ("zho", "chi", "zh", "Chinese"),
    ("kor", "kor", "ko", "Korean"),
    ("jpn", "jpn", "ja", "Japanese"),
    ("vie", "vie", "vi", "Vietnamese"),
    ("rus", "rus", "ru", "Russian"),
    ("ara", "ara", "ar", "Arabic"),
    ("hin", "hin", "hi", "Hindi"),
    ("tgl", "tgl", "tl", "Tagalog"),
    ("hat", "hat", "ht", "Haitian Creole"),
    ("pol", "pol", "pl", "Polish"),
    ("nld", "dut", "nl", "Dutch"),
    ("ell", "gre", "el", "Greek"),
    ("heb", "heb", "he", "Hebrew"),
    ("tur", "tur", "tr", "Turkish"),
    ("ukr", "ukr", "uk", "Ukrainian"),
    ("fas", "per", "fa", "Persian"),
    ("ben", "ben", "bn", "Bengali"),
    ("urd", "urd", "ur", "Urdu"),
    ("swe", "swe", "sv", "Swedish"),
    ("nor", "nor", "no", "Norwegian"),
    ("dan", "dan", "da", "Danish"),
    ("fin", "fin", "fi", "Finnish"),
];

/// A PMT language code as a tag, lower case, or None for none at all:
/// empty, `und` (undetermined) and `zxx` (no speech) say nothing a menu can use.
fn clean(code: &str) -> Option<String> {
    let c = code.trim().to_ascii_lowercase();
    let letters = (2..=3).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_lowercase());
    (letters && c != "und" && c != "zxx" && c != "mul").then_some(c)
}

fn known(code: &str) -> Option<&'static (&'static str, &'static str, &'static str, &'static str)> {
    KNOWN.iter().find(|(t, b, two, _)| *t == code || *b == code || *two == code)
}

/// `LANGUAGE` for a PMT code: the two letter tag when there is one.
pub fn tag(code: &str) -> Option<String> {
    let c = clean(code)?;
    Some(known(&c).map_or(c, |k| k.2.to_string()))
}

/// `NAME` for sound track `track` (0 the first): the language's name, the
/// code, or `Track N` counting from 1.
pub fn name(code: &str, track: u8) -> String {
    match clean(code) {
        Some(c) => known(&c).map_or(c, |k| k.3.to_string()),
        None => format!("Track {}", u32::from(track) + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pmt_code_becomes_a_tag_and_a_name() {
        assert_eq!((tag("eng"), name("eng", 0)), (Some("en".into()), "English".to_string()));
        assert_eq!((tag("fre"), name("FRA", 1)), (Some("fr".into()), "French".to_string()));
        assert_eq!((tag("qaa"), name("qaa", 2)), (Some("qaa".into()), "qaa".to_string()), "unknown: the code");
        assert_eq!((tag("und"), name("und", 2)), (None, "Track 3".to_string()));
        assert_eq!((tag(""), name("  ", 0)), (None, "Track 1".to_string()));
        assert_eq!(tag("e1g"), None);
    }
}
