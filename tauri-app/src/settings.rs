//! Where the shell keeps its state: the config the local mixer is started
//! with, the token it is started with, and which core was last connected to.
//!
//! All of it lives in the platform's application data directory, never in the
//! repository and never compiled in. The port is not kept at all: a fresh one
//! is taken at every start, so two copies of the app, or the app beside a
//! mixer someone started by hand, cannot collide on 8080.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// The daemon's config file, created on first run.
pub const CONFIG_FILE: &str = "godwinmix.toml";
const TOKEN_FILE: &str = "core-token";
const CONNECTION_FILE: &str = "connection.json";
const LOCAL_PORT_FILE: &str = "local-core.port";

/// The example config, read at compile time rather than shipped as a bundle
/// resource: it is 10 kB, it can then never go missing from an installation,
/// and `cargo run` gets the same first run as the installed app.
const EXAMPLE_CONFIG: &str = include_str!("../../godwinmix.example.toml");

/// Written above the copy so that whoever opens the file knows which two
/// settings the app is going to overrule.
const CONFIG_PREAMBLE: &str = "\
# GodwinMix desktop: the mixer's config file.
#
# Yours to edit. The canvas, the sources, the outputs and everything else
# below take effect the next time the app starts the mixer.
#
# Two lines the desktop app overrules every time it starts the mixer, because
# they belong to this machine and not to your setup:
#   [control] bind  a free port on 127.0.0.1, chosen at start
#   [control] token a random token, kept next to this file in core-token
#
# The example's two sample cameras and its sample output are commented out
# below, so the app starts on an empty desk. Uncomment them, or add sources
# and outputs from the window and let the app write them down for you.
#
# Connecting to a mixer somewhere else instead? Then none of this is used:
# that mixer reads its own config file on its own machine.

";

/// Which mixer the app talks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Start the bundled daemon here and talk to it.
    Local,
    /// Talk to one that is already running somewhere else.
    Remote,
}

/// What the operator chose last time, remembered across launches.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Connection {
    pub mode: Mode,
    /// Base URL of the remote core. Empty for `Mode::Local`.
    #[serde(default)]
    pub address: String,
    /// Bearer token for the remote core. Empty when it has none, and never
    /// used for a local core: that token is generated, not typed.
    #[serde(default)]
    pub token: String,
}

impl Default for Connection {
    fn default() -> Self {
        Self { mode: Mode::Local, address: String::new(), token: String::new() }
    }
}

/// The application data directory, created if it is not there yet.
pub fn data_dir(app: &AppHandle) -> io::Result<PathBuf> {
    let dir = app.path().app_data_dir().map_err(other)?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The application log directory, created if it is not there yet.
pub fn log_dir(app: &AppHandle) -> io::Result<PathBuf> {
    let dir = app.path().app_log_dir().map_err(other)?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The daemon's config file, written from the example on first run.
pub fn config_path(app: &AppHandle) -> io::Result<PathBuf> {
    let path = data_dir(app)?.join(CONFIG_FILE);
    if !path.exists() {
        fs::write(&path, first_run_config(EXAMPLE_CONFIG))?;
    }
    Ok(path)
}

/// The example config, ready for someone who has never run this before.
///
/// Everything down to the first `[[sources]]` is settings with defaults, and
/// is copied as it stands. From there on the example is two cameras and an
/// output pointed at an RTMP server on this machine, which nobody has on a
/// first run: left switched on they fill the window with sources that cannot
/// connect and the log with reconnect attempts. They stay, commented, because
/// the comments around them are the documentation for adding a real one.
fn first_run_config(example: &str) -> String {
    let mut out = String::with_capacity(CONFIG_PREAMBLE.len() + example.len() + 512);
    out.push_str(CONFIG_PREAMBLE);
    let mut reached_the_samples = false;
    for line in example.lines() {
        reached_the_samples |= line.starts_with("[[sources]]");
        let sample = reached_the_samples && !line.trim().is_empty() && !line.trim_start().starts_with('#');
        if sample {
            out.push_str("# ");
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The token the local daemon is started with. Generated once and kept, so
/// that a page which stored it still works after a restart.
pub fn local_token(app: &AppHandle) -> io::Result<String> {
    let path = data_dir(app)?.join(TOKEN_FILE);
    if let Ok(kept) = fs::read_to_string(&path) {
        let kept = kept.trim().to_string();
        if kept.len() >= 32 {
            return Ok(kept);
        }
    }
    let fresh = random_token()?;
    fs::write(&path, &fresh)?;
    restrict(&path);
    Ok(fresh)
}

/// 32 bytes from the operating system, as hex. Long enough that guessing it
/// is not a strategy, short enough to paste into another machine's browser.
fn random_token() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| other(format!("no randomness available: {e}")))?;
    Ok(bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    }))
}

/// Write down the port the local mixer was given, so that a shell which was
/// killed, or crashed, finds the daemon it started still running and goes
/// back to it instead of starting a second one to fight it for the encoder.
pub fn remember_local_port(app: &AppHandle, port: u16) {
    if let Ok(dir) = data_dir(app) {
        let _ = fs::write(dir.join(LOCAL_PORT_FILE), port.to_string());
    }
}

/// The port the last local mixer was given, if there was one.
pub fn last_local_port(app: &AppHandle) -> Option<u16> {
    let text = fs::read_to_string(data_dir(app).ok()?.join(LOCAL_PORT_FILE)).ok()?;
    text.trim().parse().ok()
}

/// Forget it, once that mixer has been stopped.
pub fn forget_local_port(app: &AppHandle) {
    if let Ok(dir) = data_dir(app) {
        let _ = fs::remove_file(dir.join(LOCAL_PORT_FILE));
    }
}

/// Load what was remembered. `None` on a first run, and on a file that no
/// longer parses: a stale settings file must never stop the app opening.
pub fn load(app: &AppHandle) -> Option<Connection> {
    let path = data_dir(app).ok()?.join(CONNECTION_FILE);
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Remember a connection that worked. A failure here is logged and dropped:
/// the operator is connected either way, and losing the memory is not a
/// reason to refuse the connection.
pub fn save(app: &AppHandle, conn: &Connection) {
    let Ok(dir) = data_dir(app) else { return };
    let path = dir.join(CONNECTION_FILE);
    match serde_json::to_string_pretty(conn).map_err(other).and_then(|t| fs::write(&path, t)) {
        Ok(()) => restrict(&path),
        Err(e) => eprintln!("[desktop] could not remember the connection: {e}"),
    }
}

/// Owner only, on the platforms that have the notion. Both files can hold a
/// token, and this directory is inside the user's home on all three, but a
/// shared machine is a shared machine.
#[cfg(unix)]
fn restrict(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_path: &std::path::Path) {}

fn other<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_sixty_four_hex_characters() {
        let t = random_token().expect("the operating system has randomness");
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(t, random_token().unwrap(), "two tokens in a row must differ");
    }

    #[test]
    fn the_example_config_travels_with_the_binary() {
        assert!(EXAMPLE_CONFIG.contains("[control]"), "the example must still have a control section");
        assert!(EXAMPLE_CONFIG.contains("[canvas]"));
    }

    #[test]
    fn a_first_run_config_has_no_source_switched_on() {
        let made = first_run_config(EXAMPLE_CONFIG);
        assert!(made.starts_with("# GodwinMix desktop"), "the preamble comes first");
        assert!(made.contains("\n[canvas]\n"), "settings above the samples are left alone");
        assert!(made.contains("\n[control]\n"));
        for line in made.lines() {
            assert!(
                !line.starts_with("[[sources]]") && !line.starts_with("[[outputs]]"),
                "a sample is still switched on: {line}"
            );
        }
        // Commented out, not deleted: the comments around them are how
        // someone learns what a source can be given.
        assert!(made.contains("# [[sources]]"));
        assert!(made.contains("# id = \"cam1\""));
    }

    #[test]
    fn the_preamble_says_what_the_app_overrules() {
        assert!(CONFIG_PREAMBLE.contains("bind"));
        assert!(CONFIG_PREAMBLE.contains("token"));
    }

    #[test]
    fn a_remembered_connection_round_trips() {
        let conn = Connection {
            mode: Mode::Remote,
            address: "https://studio.example:8443".into(),
            token: "abc".into(),
        };
        let text = serde_json::to_string(&conn).unwrap();
        assert!(text.contains("\"remote\""), "the mode is written in lower case: {text}");
        let back: Connection = serde_json::from_str(&text).unwrap();
        assert_eq!(back.mode, Mode::Remote);
        assert_eq!(back.address, conn.address);
    }

    #[test]
    fn a_connection_file_from_an_older_version_still_loads() {
        let back: Connection = serde_json::from_str(r#"{"mode":"local"}"#).unwrap();
        assert_eq!(back.mode, Mode::Local);
        assert!(back.address.is_empty());
    }
}
