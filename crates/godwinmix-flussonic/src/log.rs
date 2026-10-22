//! Log lines in Flussonic's format, for readers that count them, such as
//! a monitoring page that reads the v3 streams API and the log:
//!
//! ```text
//! 2026-10-23 12:00:04.123 <0.0.0> [CH117] Input closed with reason => scrambled
//! ```
//!
//! The file is rotated by size, so it cannot fill a disk: at `max_bytes` it
//! becomes `<path>.1`, the one before `<path>.2`, and the oldest kept is
//! deleted.

use crate::clock;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// What the wall counts. The words are Flussonic's own, so the same reader
/// works on either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The input carries encrypted packets.
    Scrambled,
    /// The GPU would not give the transcoder an encoder session.
    EncoderRefused,
}

impl Event {
    pub fn text(self) -> &'static str {
        match self {
            Event::Scrambled => "Input closed with reason => scrambled",
            Event::EncoderRefused => "transcoder nvenc_pin open_session configure fail: error 21, no encoder session free",
        }
    }
}

/// One line, ended with a newline.
pub fn line(at_ms: i64, stream: &str, message: &str) -> String {
    let t = clock::utc(at_ms);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} <0.0.0> [{stream}] {}\n",
        t.year, t.month, t.day, t.hour, t.minute, t.second, t.milli,
        message.replace(['\n', '\r'], " ")
    )
}

pub struct Log {
    path: PathBuf,
    max_bytes: u64,
    keep: u32,
    file: Option<File>,
    size: u64,
}

impl Log {
    /// Ten megabytes, and three old files: enough for the wall's ten
    /// minute window many times over.
    pub fn new(path: &Path) -> Log {
        Log::with_limits(path, 10 << 20, 3)
    }

    pub fn with_limits(path: &Path, max_bytes: u64, keep: u32) -> Log {
        Log { path: path.to_path_buf(), max_bytes, keep: keep.max(1), file: None, size: 0 }
    }

    fn rotated(&self, n: u32) -> PathBuf {
        let mut p = self.path.clone().into_os_string();
        p.push(format!(".{n}"));
        PathBuf::from(p)
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        self.file = None;
        let _ = std::fs::remove_file(self.rotated(self.keep));
        for n in (1..self.keep).rev() {
            let _ = std::fs::rename(self.rotated(n), self.rotated(n + 1));
        }
        std::fs::rename(&self.path, self.rotated(1))
    }

    fn open(&mut self) -> std::io::Result<&mut File> {
        if self.file.is_none() {
            if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir)?;
            }
            let f = OpenOptions::new().create(true).append(true).open(&self.path)?;
            self.size = f.metadata().map(|m| m.len()).unwrap_or(0);
            self.file = Some(f);
        }
        Ok(self.file.as_mut().expect("opened above"))
    }

    pub fn write(&mut self, stream: &str, message: &str) -> std::io::Result<()> {
        let text = line(clock::now_ms(), stream, message);
        self.open()?;
        if self.size + text.len() as u64 > self.max_bytes && self.size > 0 {
            self.rotate()?;
            self.open()?;
        }
        let f = self.file.as_mut().expect("opened above");
        f.write_all(text.as_bytes())?;
        self.size += text.len() as u64;
        Ok(())
    }

    pub fn event(&mut self, stream: &str, e: Event) -> std::io::Result<()> {
        self.write(stream, e.text())
    }
}

#[cfg(test)]
mod tests;
