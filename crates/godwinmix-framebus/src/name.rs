//! What a frame bus is called: `camera:<id>` or `channel:<app>/<stream>`.

use std::fmt;
use std::str::FromStr;

use crate::Error;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BusName {
    /// A device this machine opens: a camera, a capture card.
    Camera(String),
    /// A stream arriving on a channel, as `<app>/<stream>`.
    Channel { app: String, stream: String },
}

/// Ids are slugs. Stream names come from encoders, so they may also carry
/// capitals and underscores (`main_720p`); nothing that means something in
/// a path or in the file name below.
fn valid(part: &str) -> bool {
    (1..=64).contains(&part.len())
        && part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl BusName {
    pub fn camera(id: &str) -> Result<BusName, Error> {
        format!("camera:{id}").parse()
    }

    pub fn channel(app: &str, stream: &str) -> Result<BusName, Error> {
        format!("channel:{app}/{stream}").parse()
    }

    /// The socket's file name in the registry directory. `=` and `+` never
    /// appear in a part, so two names never share a file.
    pub fn file_name(&self) -> String {
        match self {
            BusName::Camera(id) => format!("camera={id}.sock"),
            BusName::Channel { app, stream } => format!("channel={app}+{stream}.sock"),
        }
    }

    /// The name back from a file name, or `None` for a file that is not one.
    pub fn from_file_name(file: &str) -> Option<BusName> {
        let stem = file.strip_suffix(".sock")?;
        let (kind, rest) = stem.split_once('=')?;
        let text = match kind {
            "camera" => format!("camera:{rest}"),
            "channel" => format!("channel:{}", rest.replacen('+', "/", 1)),
            _ => return None,
        };
        text.parse().ok()
    }
}

impl FromStr for BusName {
    type Err = Error;

    fn from_str(s: &str) -> Result<BusName, Error> {
        let bad = || {
            Error::BadName(format!(
                "'{s}' is not a frame bus name. Use camera:<id> or channel:<app>/<stream>, \
                 each part 1 to 64 letters, digits, '-' or '_'"
            ))
        };
        match s.split_once(':').ok_or_else(bad)? {
            ("camera", id) if valid(id) => Ok(BusName::Camera(id.into())),
            ("channel", rest) => match rest.split_once('/') {
                Some((app, stream)) if valid(app) && valid(stream) => Ok(BusName::Channel {
                    app: app.into(),
                    stream: stream.into(),
                }),
                _ => Err(bad()),
            },
            _ => Err(bad()),
        }
    }
}

impl fmt::Display for BusName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BusName::Camera(id) => write!(f, "camera:{id}"),
            BusName::Channel { app, stream } => write!(f, "channel:{app}/{stream}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_print_and_survive_the_file_system() {
        for text in ["camera:cam-wide", "channel:sunday-service/main_720p"] {
            let n: BusName = text.parse().unwrap();
            assert_eq!(n.to_string(), text);
            assert_eq!(BusName::from_file_name(&n.file_name()), Some(n));
        }
        assert_eq!(
            BusName::channel("a", "b").unwrap().file_name(),
            "channel=a+b.sock"
        );
    }

    #[test]
    fn a_bad_name_says_what_a_good_one_looks_like() {
        for text in [
            "cam-wide",
            "camera:",
            "camera:../x",
            "channel:app",
            "screen:1",
            "camera:a b",
        ] {
            let e = text.parse::<BusName>().unwrap_err();
            assert!(e.to_string().contains("camera:<id>"), "{e}");
        }
        assert_eq!(BusName::from_file_name("notes.txt"), None);
    }
}
