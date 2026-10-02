//! What each piece is called in a person's words.
//!
//! A plugin's name is an id (`audio-device`), which is right for a log and
//! wrong for a sentence. These are the words the picker already uses for the
//! same things, so the message and the button agree with the page.

/// The piece the browser renderer is set up as.
pub const WEB: &str = "web";

/// `(piece, plural noun for a sentence, title for a heading)`.
const TABLE: &[(&str, &str, &str)] = &[
    (WEB, "web pages", "Web pages"),
    ("camera", "cameras", "Cameras"),
    ("screen", "screen capture", "Screen capture"),
    ("audio-device", "microphones and sound cards", "Microphones and audio"),
    ("ingest", "channels and browser cameras", "Channels"),
    ("whip", "WebRTC sources", "WebRTC sources"),
    ("ndi", "NDI sources", "NDI"),
    ("decklink", "capture cards", "Capture cards"),
    ("srt", "SRT streams", "SRT"),
    ("udp", "network streams", "Network streams"),
    ("rtsp", "RTSP cameras", "RTSP cameras"),
    ("ipcam", "network cameras", "Network cameras"),
    ("ograf", "graphics", "Graphics"),
    ("file-record", "recording", "Recording"),
    ("icecast", "radio streams", "Radio streams"),
    ("osc", "remote control", "Remote control"),
    ("tally", "tally lights", "Tally lights"),
    ("director", "the automatic director", "Automatic director"),
    ("wipe", "wipe transitions", "Wipe transitions"),
];

/// "cameras", for the middle of a sentence. A piece this table does not know
/// is "this feature", which reads in every sentence it is put in.
pub fn noun(piece: &str) -> &'static str {
    TABLE.iter().find(|(p, ..)| *p == piece).map(|(_, n, _)| *n).unwrap_or("this feature")
}

/// "Cameras", for a heading or the start of a sentence.
pub fn title(piece: &str) -> &'static str {
    TABLE.iter().find(|(p, ..)| *p == piece).map(|(.., t)| *t).unwrap_or("This feature")
}

/// The same noun with a capital, for the start of a sentence.
pub fn sentence_start(piece: &str) -> String {
    let n = noun(piece);
    let mut c = n.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A plugin that ships with the mixer and has words here.
pub fn known(piece: &str) -> bool {
    TABLE.iter().any(|(p, ..)| *p == piece)
}

/// Every first party plugin with words here, which is every one the mixer
/// may set up by itself.
pub fn plugins() -> impl Iterator<Item = &'static str> {
    TABLE.iter().map(|(p, ..)| *p).filter(|p| *p != WEB)
}
