//! What this terminal can actually do.
//!
//! Nothing here assumes. The environment gives a definite answer for the
//! terminals that publish one, and otherwise the terminal is asked: a kitty
//! graphics query and a primary device attributes request go out together, and
//! whichever answer comes back decides. A terminal that answers neither, or a
//! run that is not on a terminal at all, gets the block grid.

use crate::picture::Picture;
#[cfg(unix)]
use std::time::Duration;

/// How long to wait for a terminal to answer. A real terminal answers a
/// primary device attributes request in microseconds; this is only here so
/// that something which never answers does not hold the mixer's UI up.
#[cfg(unix)]
const REPLY_WAIT: Duration = Duration::from_millis(400);

/// The picture mode to use. `force` is `--picture`, which skips the lot.
pub fn detect(force: Option<Picture>) -> Picture {
    if let Some(forced) = force {
        return forced;
    }
    if let Some(from_env) = from_env() {
        return from_env;
    }
    query().unwrap_or(Picture::Blocks)
}

/// The terminals that say what they are in the environment.
fn from_env() -> Option<Picture> {
    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    let program = std::env::var("TERM_PROGRAM").unwrap_or_default().to_lowercase();
    if std::env::var_os("KITTY_WINDOW_ID").is_some() || term.contains("kitty") {
        return Some(Picture::Kitty);
    }
    if program == "ghostty" || std::env::var_os("GHOSTTY_RESOURCES_DIR").is_some() {
        return Some(Picture::Kitty);
    }
    if std::env::var_os("WEZTERM_PANE").is_some() || program == "wezterm" {
        return Some(Picture::Kitty);
    }
    if program == "iterm.app" {
        return Some(Picture::Sixel);
    }
    if term.starts_with("foot") || term.contains("mlterm") || term.contains("yaft") {
        return Some(Picture::Sixel);
    }
    None
}

/// Ask the terminal. Unix only: reading the raw reply needs stdin in raw mode
/// carrying the bytes the terminal wrote, which is what a Unix tty does and
/// what a Windows console does not without more machinery than a picture is
/// worth. Windows falls through to the environment and to `--picture`, both of
/// which are documented in `docs/how-to/terminal-ui.md`.
#[cfg(unix)]
fn query() -> Option<Picture> {
    use crossterm::tty::IsTty;
    use std::io::{Read, Write};
    let mut out = std::io::stdout();
    if !out.is_tty() {
        return None;
    }
    // The kitty query first, then primary device attributes. Every terminal
    // answers the second one, which is what makes the read finish rather than
    // wait for the timeout.
    out.write_all(b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c").ok()?;
    out.flush().ok()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buffer = [0u8; 128];
        let mut seen: Vec<u8> = Vec::new();
        while seen.len() < 4096 {
            match stdin.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    seen.extend_from_slice(&buffer[..n]);
                    // The device attributes reply ends in `c`, and neither the
                    // kitty reply nor an escape prefix contains one.
                    if seen.contains(&b'c') {
                        break;
                    }
                }
            }
        }
        let _ = tx.send(seen);
    });
    let reply = rx.recv_timeout(REPLY_WAIT).ok()?;
    read_reply(&reply)
}

#[cfg(not(unix))]
fn query() -> Option<Picture> {
    None
}

/// What the terminal said. Kitty answers the graphics query with `OK`; a
/// terminal that can do sixel lists attribute 4 in its device attributes.
pub fn read_reply(bytes: &[u8]) -> Option<Picture> {
    let text = String::from_utf8_lossy(bytes);
    if text.contains("_Gi=31;OK") {
        return Some(Picture::Kitty);
    }
    let attributes = text.split("\x1b[?").nth(1)?;
    let attributes = attributes.split('c').next()?;
    if attributes.split(';').any(|a| a == "4") {
        return Some(Picture::Sixel);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_says_ok() {
        assert_eq!(read_reply(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;c"), Some(Picture::Kitty));
    }

    #[test]
    fn sixel_is_attribute_four() {
        assert_eq!(read_reply(b"\x1b[?62;4;6;9;15c"), Some(Picture::Sixel));
    }

    #[test]
    fn a_plain_terminal_says_neither() {
        assert_eq!(read_reply(b"\x1b[?62;22c"), None);
        assert_eq!(read_reply(b""), None);
        // 64 is not 4, and a substring match would have got this wrong.
        assert_eq!(read_reply(b"\x1b[?64;22c"), None);
    }
}
