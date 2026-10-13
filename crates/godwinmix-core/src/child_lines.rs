//! A child's stderr, one line at a time, as text.
//!
//! A plugin's JSON lines channel is UTF-8 by the protocol, and so is every
//! log line the core keeps. A line that is not used to end the Windows reader
//! outright (`lines()` answers an error for it, and the loop stopped there),
//! which left a plugin running with nobody reading its replies. Now such a
//! line is reported once, naming the child and the byte, and handed on with
//! the bad bytes replaced, so the reply it carried still arrives and the
//! channel goes on.

use std::io::BufRead;
use tracing::warn;

/// Split `pending` at each new line and hand every whole line on. What is
/// left is a line still being written.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn split(name: &str, pending: &mut Vec<u8>, on_line: &mut impl FnMut(&str)) {
    while let Some(nl) = pending.iter().position(|b| *b == b'\n') {
        let line: Vec<u8> = pending.drain(..=nl).collect();
        deliver(name, &line[..nl], on_line);
    }
}

/// Every line `reader` gives until it ends.
#[cfg_attr(unix, allow(dead_code))]
pub(crate) fn read_all(name: &str, reader: impl std::io::Read, on_line: &mut impl FnMut(&str)) {
    let mut reader = std::io::BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let end = line.len() - usize::from(line.last() == Some(&b'\n'));
                deliver(name, &line[..end], on_line);
            }
        }
    }
}

/// One line, as text.
pub(crate) fn deliver(name: &str, line: &[u8], on_line: &mut impl FnMut(&str)) {
    match std::str::from_utf8(line) {
        Ok(text) => on_line(text.trim_end()),
        Err(e) => {
            warn!(
                child = name,
                at_byte = e.valid_up_to(),
                "{} wrote a line that is not UTF-8 (the first bad byte is at {}). The plugin \
                 channel is UTF-8: a Python plugin on Windows writes the local code page \
                 unless PYTHONUTF8=1, which the core sets for it unless its own environment \
                 says otherwise. The line was read with the bad bytes replaced.",
                who(name),
                e.valid_up_to()
            );
            on_line(String::from_utf8_lossy(line).trim_end());
        }
    }
}

/// `plugin-cam1` is the plugin instance `cam1`; anything else is named as is.
fn who(name: &str) -> String {
    match name.strip_prefix("plugin-") {
        Some(instance) => format!("the plugin instance `{instance}`"),
        None => format!("`{name}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_of(bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        read_all("plugin-scripture-service", bytes, &mut |l: &str| out.push(l.to_string()));
        out
    }

    #[test]
    fn a_line_in_the_local_code_page_is_read_and_the_lines_after_it_still_arrive() {
        // "Lord’s" in cp1252: the right quote is the single byte 0x92.
        let got = lines_of(b"{\"text\":\"Lord\x92s\"}\r\n{\"id\":2}\n\xe2\x80\x9cquoted\xe2\x80\x9d");
        assert_eq!(got, vec!["{\"text\":\"Lord\u{fffd}s\"}", "{\"id\":2}", "\u{201c}quoted\u{201d}"]);
    }

    #[test]
    fn split_keeps_a_partial_line_for_later() {
        let mut pending = b"one\ntw".to_vec();
        let mut out = Vec::new();
        split("x", &mut pending, &mut |l: &str| out.push(l.to_string()));
        assert_eq!(out, vec!["one"]);
        assert_eq!(pending, b"tw");
    }
}
