//! Turning a pattern into the file name `splitmuxsink` writes.
//!
//! Two jobs, and the second is the one that matters. The first is expanding
//! `{date}`, `{time}`, `{datetime}` and `{instance}` so an operator gets
//! `sunday-2026-09-13-1030.mp4` rather than `recording.mp4` over the top of
//! last week's.
//!
//! The second is that `splitmuxsink` hands the location to `printf` with the
//! fragment number. A path an operator typed is not a format string, so a
//! stray `%` in a folder name would at best produce nonsense and at worst read
//! memory that is not there. Every `%` from the pattern is doubled, and the
//! one conversion `splitmuxsink` needs is added here, deliberately, where it
//! can be seen.

use gstreamer::glib;

/// The printf conversion `splitmuxsink` fills in with the fragment number.
const INDEX: &str = "%05d";

/// The name of the first file, and the template for the rest.
///
/// `now` is passed in rather than read, so the tests do not depend on the
/// clock.
pub fn location(
    pattern: &str,
    instance: &str,
    extension: &str,
    split: bool,
    now: &Stamp,
) -> String {
    let expanded = expand(pattern, instance, now);
    let safe = escape_percent(&expanded);
    if split {
        format!("{safe}-{INDEX}.{extension}")
    } else {
        format!("{safe}.{extension}")
    }
}

/// The date and time a name is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    /// `2026-09-13`.
    pub date: String,
    /// `1030`.
    pub time: String,
}

impl Stamp {
    /// The local date and time now.
    ///
    /// Local, not UTC: a recording is named for the service it is of, and the
    /// person looking for it is in the room, not at Greenwich. `glib` is
    /// already in the tree behind GStreamer and knows the machine's timezone,
    /// so this costs no dependency.
    pub fn now() -> Stamp {
        match glib::DateTime::now_local() {
            Ok(t) => Stamp {
                date: t
                    .format("%Y-%m-%d")
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                time: t.format("%H%M").map(|s| s.to_string()).unwrap_or_default(),
            },
            // A machine whose clock will not answer still records; it gets a
            // name without a date rather than no recording.
            Err(_) => Stamp {
                date: "undated".into(),
                time: "0000".into(),
            },
        }
    }

    pub fn datetime(&self) -> String {
        format!("{}-{}", self.date, self.time)
    }
}

fn expand(pattern: &str, instance: &str, now: &Stamp) -> String {
    pattern
        .replace("{datetime}", &now.datetime())
        .replace("{date}", &now.date)
        .replace("{time}", &now.time)
        .replace("{instance}", instance)
}

/// Double every `%`, so nothing an operator typed is read as a conversion.
fn escape_percent(text: &str) -> String {
    text.replace('%', "%%")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp() -> Stamp {
        Stamp {
            date: "2026-09-13".into(),
            time: "1030".into(),
        }
    }

    #[test]
    fn a_pattern_becomes_a_name_a_person_can_read() {
        assert_eq!(
            location("{instance}-{datetime}", "archive", "mp4", false, &stamp()),
            "archive-2026-09-13-1030.mp4"
        );
        assert_eq!(
            location("sunday-{date}", "archive", "mkv", false, &stamp()),
            "sunday-2026-09-13.mkv"
        );
        assert_eq!(location("{time}", "a", "mp4", false, &stamp()), "1030.mp4");
    }

    #[test]
    fn a_split_recording_gets_a_number_the_muxer_fills_in() {
        let name = location("service-{date}", "archive", "mp4", true, &stamp());
        assert_eq!(name, "service-2026-09-13-%05d.mp4");
        assert!(name.contains(INDEX));
    }

    #[test]
    fn a_folder_with_a_percent_in_it_is_not_a_format_string() {
        // The reason this module exists. `splitmuxsink` prints the location
        // with the fragment number, so a name carrying `%s` would be read as a
        // conversion and print whatever happened to be next on the stack.
        let name = location("100%-live", "a", "mp4", false, &stamp());
        assert_eq!(name, "100%%-live.mp4");
        // Every conversion an operator could have typed is doubled, which is
        // how printf is told to print a literal per cent sign.
        let sneaky = location("{instance}%s%n", "a", "mp4", false, &stamp());
        assert_eq!(sneaky, "a%%s%%n.mp4");
        assert_eq!(
            sneaky.matches('%').count() % 2,
            0,
            "every % is paired: {sneaky}"
        );
    }

    #[test]
    fn a_pattern_with_no_tokens_is_left_alone() {
        assert_eq!(
            location("recording", "a", "mp4", false, &stamp()),
            "recording.mp4"
        );
    }

    #[test]
    fn a_pattern_may_put_the_date_in_a_folder() {
        assert_eq!(
            location("{date}/service-{time}", "a", "mp4", false, &stamp()),
            "2026-09-13/service-1030.mp4"
        );
    }

    #[test]
    fn the_clock_gives_a_date_and_a_time_that_look_like_ones() {
        let now = Stamp::now();
        assert_eq!(now.date.len(), 10, "{}", now.date);
        assert_eq!(now.time.len(), 4, "{}", now.time);
        assert!(now.datetime().starts_with(&now.date));
    }
}
