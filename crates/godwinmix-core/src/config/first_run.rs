//! A missing config file is a first run, not an error.
//!
//! Someone who unpacked the release and typed `godwinmix` has no config yet,
//! and the desktop app starting its own mixer for the first time has none
//! either. Both get the same file: the example, with its sample cameras and
//! its sample outputs commented out. Left switched on, those samples point at
//! an RTMP server on 127.0.0.1 that nobody has on a first run, so the window
//! fills with sources that cannot connect and the welcome tiles never show,
//! because they only show on a mixer with no sources.
//!
//! The samples stay in the file, commented, because the comments around them
//! are the documentation for adding a real one.

use std::io;
use std::path::Path;

/// The example config, read at compile time so it can never go missing from
/// an installation. `godwinmix --example-config` prints the first run form of
/// it, and a first run writes that same text.
pub const EXAMPLE_CONFIG: &str = include_str!("../../../../godwinmix.example.toml");

/// Written above the copy, so whoever opens the file knows where it came from
/// and which two lines the desktop app overrules when it is the one starting
/// the mixer.
pub const PREAMBLE: &str = "\
# GodwinMix: the mixer's config file.
#
# Written on the first start, because there was none. Yours to edit: the
# canvas, the sources, the outputs and everything else below take effect the
# next time the mixer starts.
#
# The example's sample cameras and sample outputs are commented out below, so
# the mixer starts on an empty desk and the page offers its welcome tiles.
# Uncomment them, or add sources and outputs from the page and let the mixer
# write them down for you.
#
# When the desktop app starts this mixer it overrules two lines, because they
# belong to that machine and not to your setup:
#   [control] bind  a free port on 127.0.0.1, chosen at start
#   [control] token a random token, kept next to this file in core-token

";

/// The example, ready for someone who has never run this before: every
/// `[[sources]]` and `[[outputs]]` table commented out, with the tables that
/// hang off them (`[sources.params]` and the like), and everything else as it
/// stands, in whatever order the example has them.
pub fn first_run_config(example: &str) -> String {
    let mut out = String::with_capacity(PREAMBLE.len() + example.len() + 512);
    out.push_str(PREAMBLE);
    let mut in_sample = false;
    for line in example.lines() {
        if let Some(name) = table_name(line) {
            in_sample = is_sample_table(name);
        }
        let trimmed = line.trim();
        if in_sample && !trimmed.is_empty() && !trimmed.starts_with('#') {
            out.push_str("# ");
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The name inside a table header, `sources.params` for `[sources.params]`
/// and `sources` for `[[sources]]`. `None` for any other line.
fn table_name(line: &str) -> Option<&str> {
    let t = line.trim();
    let inner = t
        .strip_prefix("[[")
        .and_then(|r| r.strip_suffix("]]"))
        .or_else(|| t.strip_prefix('[').and_then(|r| r.strip_suffix(']')))?;
    Some(inner.trim())
}

fn is_sample_table(name: &str) -> bool {
    ["sources", "outputs"].iter().any(|root| {
        name == *root || name.strip_prefix(root).is_some_and(|rest| rest.starts_with('.'))
    })
}

/// Write the first run config at `path` when nothing is there. `Ok(true)` when
/// it wrote one, `Ok(false)` when a file was already there, which is left
/// alone. The folder has to exist already: a config path whose folder is
/// missing is more likely a typo than a first run.
pub fn write_if_missing(path: &Path) -> io::Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    let text = first_run_config(EXAMPLE_CONFIG);
    match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(text.as_bytes())?;
            Ok(true)
        }
        // Two starts at once: the other one wrote it, which is just as good.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_run_config_has_no_source_or_output_switched_on() {
        let made = first_run_config(EXAMPLE_CONFIG);
        assert!(made.starts_with("# GodwinMix: the mixer's config file."));
        assert!(made.contains("\n[canvas]\n"), "settings are left alone");
        assert!(made.contains("\n[control]\n"));
        for line in made.lines() {
            let t = line.trim_start();
            assert!(
                !t.starts_with("[[sources]]") && !t.starts_with("[[outputs]]") && !t.starts_with("[sources."),
                "a sample is still switched on: {line}"
            );
        }
        assert!(made.contains("# [[sources]]"), "commented out, not deleted");
        let cfg: crate::config::Config = toml::from_str(&made).expect("the first run config parses");
        assert!(cfg.sources.is_empty() && cfg.outputs.is_empty());
    }

    #[test]
    fn samples_are_found_in_any_order_and_what_follows_them_stays_on() {
        let example = "[canvas]\nwidth = 1280\n\n[[outputs]]\nid = \"a\"\n\n[stall]\nsecs = 3\n\n\
                       [[sources]]\nid = \"cam1\"\n[sources.params]\nx = 1\n\n[sourcesish]\ny = 2\n";
        let made = first_run_config(example);
        assert!(made.contains("# [[outputs]]\n# id = \"a\""));
        assert!(made.contains("\n[stall]\nsecs = 3"), "a table after a sample is still live");
        assert!(made.contains("# [sources.params]\n# x = 1"));
        assert!(made.contains("\n[sourcesish]\ny = 2"), "only sources and its own subtables");
    }

    #[test]
    fn a_first_run_writes_once_and_never_over_a_file() {
        let dir = std::env::temp_dir().join(format!("gmx-first-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("godwinmix.toml");
        assert!(write_if_missing(&path).unwrap(), "nothing there, so it writes");
        crate::config::Config::load(&path).expect("what it wrote loads");
        std::fs::write(&path, "# mine\n").unwrap();
        assert!(!write_if_missing(&path).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# mine\n", "a file that is there is left alone");
        assert!(write_if_missing(&dir.join("no/such/folder.toml")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
