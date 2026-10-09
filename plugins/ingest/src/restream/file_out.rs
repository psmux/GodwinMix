//! Recording to a file: the tags remuxed to MPEG-TS by `crate::tsmux` and
//! written as they come. MPEG-TS because a recording cut short by a power
//! cut is still a file every player opens, with no index to finish.
//!
//! `file:///recordings/bbc-one.ts`. A file already there is never written
//! over: the new one gets the time it started in its name. Each
//! connection (a reconnect after a full disk, say) starts a new file.
//!
//! A channel's `file` destination names its files with `{time}` in the
//! address (`file:///C:/Videos/GodwinMix/sunday-main-{time}.ts`), which
//! becomes the local time the file was opened, `20261009-103000`, so every
//! time the stream goes live there is a new file named for when it began.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::media_tag::MediaTag;
use crate::tsmux::Muxer;

use super::link::{Failure, Link};
use super::target::Target;

pub struct FileLink {
    out: BufWriter<File>,
    muxer: Muxer,
    buf: Vec<u8>,
    path: PathBuf,
}

/// The path out of `file:///a/b.ts`, or the address itself when it has no
/// scheme. `file:///C:/a/b.ts` is `C:/a/b.ts`: the slash before a drive
/// letter is the URL's, not the path's.
pub fn path_of(url: &str) -> PathBuf {
    let p = url.strip_prefix("file://").unwrap_or(url);
    let p = p.split('?').next().unwrap_or(p);
    let drive = p.as_bytes();
    let p = if drive.len() > 2 && drive[0] == b'/' && drive[1].is_ascii_alphabetic() && drive[2] == b':' { &p[1..] } else { p };
    PathBuf::from(p)
}

/// The local time now, as a file name carries it: `20261009-103000`.
pub fn stamp() -> String {
    glib::DateTime::now_local()
        .and_then(|now| now.format("%Y%m%d-%H%M%S"))
        .map(|s| s.to_string())
        .unwrap_or_else(|_| {
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            secs.to_string()
        })
}

/// `path`, or, when that is taken, the same name with the time in it.
fn free(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("recording");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("ts");
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut n = 0;
    loop {
        let name = if n == 0 { format!("{stem}-{secs}.{ext}") } else { format!("{stem}-{secs}-{n}.{ext}") };
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

impl FileLink {
    pub fn dial(target: &Target) -> Result<FileLink, Failure> {
        let wanted = path_of(&target.url.replace("{time}", &stamp()));
        if wanted.as_os_str().is_empty() || wanted.is_dir() {
            return Err(Failure::Refused(format!(
                "'{}' is not a file to record to. Give the whole path with a name, as in file:///recordings/show.ts",
                wanted.display()
            )));
        }
        if let Some(dir) = wanted.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| Failure::Refused(format!("cannot make the folder {}: {e}", dir.display())))?;
        }
        let path = free(&wanted);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| Failure::Lost(format!("cannot write {}: {e}", path.display())))?;
        Ok(FileLink { out: BufWriter::with_capacity(256 * 1024, file), muxer: Muxer::new(), buf: Vec::with_capacity(64 * 1024), path })
    }
}

impl Link for FileLink {
    fn send(&mut self, tag: &MediaTag, timestamp_ms: u32) -> Result<usize, Failure> {
        self.muxer.tag(tag, timestamp_ms, &mut self.buf);
        let n = self.buf.len();
        let written = self.out.write_all(&self.buf).and_then(|_| if tag.keyframe { self.out.flush() } else { Ok(()) });
        self.buf.clear();
        written.map_err(|e| Failure::Lost(format!("writing {} failed: {e}", self.path.display())))?;
        Ok(n)
    }

    fn poll(&mut self) -> Result<(), Failure> {
        Ok(())
    }

    fn close(&mut self) {
        let _ = self.out.flush();
    }

    fn file(&self) -> Option<&Path> {
        Some(&self.path)
    }
}

impl Drop for FileLink {
    fn drop(&mut self) {
        let _ = self.out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drive_letter_keeps_its_place_and_a_unix_path_its_slash() {
        assert_eq!(path_of("file:///C:/Videos/a.ts"), PathBuf::from("C:/Videos/a.ts"));
        assert_eq!(path_of("file:///recordings/a.ts?x=1"), PathBuf::from("/recordings/a.ts"));
        assert_eq!(path_of("D:/rec/a.ts"), PathBuf::from("D:/rec/a.ts"));
    }

    #[test]
    fn the_stamp_is_the_date_then_the_time_to_the_second() {
        let s = stamp();
        assert_eq!(s.len(), 15, "{s}");
        assert_eq!(&s[8..9], "-", "{s}");
        assert!(s.chars().filter(|c| *c != '-').all(|c| c.is_ascii_digit()), "{s}");
    }
}
