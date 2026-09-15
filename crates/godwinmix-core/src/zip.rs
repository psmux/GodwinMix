//! A store only zip, read and written here rather than from a crate.
//!
//! A zip with no compression is a local header, the bytes, and a central
//! directory: about two hundred lines with the CRC and the reader. Every zip
//! tool on every platform opens one, and the `zip` crates pull in a
//! compressor, a time crate and often an encryption backend. The support
//! bundle wrote the first copy of this (`gmx support-bundle`); a collection
//! export wants the same writer and a reader beside it, so both live here and
//! the bundle uses them.
//!
//! What it does not do: zip64, encryption, compressed members. A member over
//! 4 GiB is refused rather than written wrongly, and a compressed member read
//! back names the method and says what wrote it. A collection is JSON and
//! pictures, so nothing here is a limit anybody meets.

use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Signatures, so the arithmetic below reads as the format does.
const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
/// The largest member this writer will take. Past it a zip needs zip64, and
/// silently writing a wrapped length is how an archive becomes unreadable
/// months later.
const MAX_MEMBER: usize = u32::MAX as usize;

/// A zip archive built in memory, stored not deflated.
pub struct Zip {
    out: Vec<u8>,
    entries: Vec<Entry>,
}

struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

impl Default for Zip {
    fn default() -> Self {
        Self::new()
    }
}

impl Zip {
    pub fn new() -> Self {
        Self { out: Vec::new(), entries: Vec::new() }
    }

    /// Add one file. `name` is the path inside the archive, forward slashes,
    /// as the zip format requires on every platform including Windows.
    ///
    /// A name already in the archive is refused and the first one stands.
    /// `unzip` asks the operator what to do about a duplicate, and an archive
    /// is a thing people unpack in a script. A member too large for a
    /// non zip64 archive is refused for the same reason.
    pub fn add(&mut self, name: &str, data: &[u8]) -> bool {
        let name = name.replace('\\', "/");
        if data.len() > MAX_MEMBER || self.entries.iter().any(|e| e.name == name) {
            return false;
        }
        let crc = crc32(data);
        let offset = self.out.len() as u32;
        self.out.extend_from_slice(&LOCAL.to_le_bytes());
        self.out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        self.out.extend_from_slice(&0u16.to_le_bytes()); // flags
        self.out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        self.out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        self.out.extend_from_slice(&0x21u16.to_le_bytes()); // mod date: 1980-01-01
        self.out.extend_from_slice(&crc.to_le_bytes());
        self.out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        self.out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        self.out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes()); // extra length
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(data);
        self.entries.push(Entry { name, crc, size: data.len() as u32, offset });
        true
    }

    /// The finished archive.
    pub fn finish(mut self) -> Vec<u8> {
        let directory_at = self.out.len() as u32;
        for e in &self.entries {
            self.out.extend_from_slice(&CENTRAL.to_le_bytes());
            self.out.extend_from_slice(&20u16.to_le_bytes()); // version made by
            self.out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            self.out.extend_from_slice(&0u16.to_le_bytes()); // flags
            self.out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
            self.out.extend_from_slice(&0u16.to_le_bytes()); // mod time
            self.out.extend_from_slice(&0x21u16.to_le_bytes()); // mod date
            self.out.extend_from_slice(&e.crc.to_le_bytes());
            self.out.extend_from_slice(&e.size.to_le_bytes());
            self.out.extend_from_slice(&e.size.to_le_bytes());
            self.out.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
            self.out.extend_from_slice(&0u16.to_le_bytes()); // extra
            self.out.extend_from_slice(&0u16.to_le_bytes()); // comment
            self.out.extend_from_slice(&0u16.to_le_bytes()); // disk number
            self.out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            self.out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            self.out.extend_from_slice(&e.offset.to_le_bytes());
            self.out.extend_from_slice(e.name.as_bytes());
        }
        let directory_size = self.out.len() as u32 - directory_at;
        let count = self.entries.len() as u16;
        self.out.extend_from_slice(&END.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes()); // this disk
        self.out.extend_from_slice(&0u16.to_le_bytes()); // directory's disk
        self.out.extend_from_slice(&count.to_le_bytes());
        self.out.extend_from_slice(&count.to_le_bytes());
        self.out.extend_from_slice(&directory_size.to_le_bytes());
        self.out.extend_from_slice(&directory_at.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes()); // comment length
        self.out
    }
}

/// Read a store only zip into its members, keyed by the path inside it.
///
/// The central directory is the index, so it is what is walked: a local header
/// can lie about its sizes when the streaming bit is set, and the directory
/// never does. Every failure names what was wrong with the file rather than
/// the offset it happened at, because the person reading the message has the
/// file and not the format.
pub fn read(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, ZipError> {
    let end = find_end(bytes).ok_or(ZipError::NotAZip)?;
    let count = u16::from_le_bytes([bytes[end + 10], bytes[end + 11]]) as usize;
    let mut at = u32::from_le_bytes([
        bytes[end + 16],
        bytes[end + 17],
        bytes[end + 18],
        bytes[end + 19],
    ]) as usize;
    let mut out = BTreeMap::new();
    for _ in 0..count {
        let header = bytes.get(at..at + 46).ok_or(ZipError::Truncated)?;
        if u32::from_le_bytes([header[0], header[1], header[2], header[3]]) != CENTRAL {
            return Err(ZipError::Truncated);
        }
        let method = u16::from_le_bytes([header[10], header[11]]);
        let size = u32::from_le_bytes([header[24], header[25], header[26], header[27]]) as usize;
        let name_len = u16::from_le_bytes([header[28], header[29]]) as usize;
        let extra_len = u16::from_le_bytes([header[30], header[31]]) as usize;
        let comment_len = u16::from_le_bytes([header[32], header[33]]) as usize;
        let offset =
            u32::from_le_bytes([header[42], header[43], header[44], header[45]]) as usize;
        let name = bytes.get(at + 46..at + 46 + name_len).ok_or(ZipError::Truncated)?;
        let name = String::from_utf8_lossy(name).into_owned();
        at += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') {
            // A directory entry carries no bytes. The paths do the nesting.
            continue;
        }
        if method != 0 {
            return Err(ZipError::Compressed { name, method });
        }
        let local = bytes.get(offset..offset + 30).ok_or(ZipError::Truncated)?;
        if u32::from_le_bytes([local[0], local[1], local[2], local[3]]) != LOCAL {
            return Err(ZipError::Truncated);
        }
        // The local header has its own name and extra lengths, and they are
        // allowed to differ from the directory's. Padding an archive by
        // growing the local extra field is a real thing signing tools do.
        let local_name = u16::from_le_bytes([local[26], local[27]]) as usize;
        let local_extra = u16::from_le_bytes([local[28], local[29]]) as usize;
        let from = offset + 30 + local_name + local_extra;
        let data = bytes.get(from..from + size).ok_or(ZipError::Truncated)?;
        if crc32(data) != u32::from_le_bytes([header[16], header[17], header[18], header[19]]) {
            return Err(ZipError::Corrupt { name });
        }
        out.insert(name, data.to_vec());
    }
    Ok(out)
}

/// What went wrong with a file somebody handed us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZipError {
    NotAZip,
    Truncated,
    Corrupt { name: String },
    Compressed { name: String, method: u16 },
}

impl std::fmt::Display for ZipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZipError::NotAZip => write!(
                f,
                "this file is not a zip archive: it has no central directory. \
                 Pass the .zip that scene.export wrote, or the directory it unpacks to."
            ),
            ZipError::Truncated => write!(
                f,
                "this zip archive stops in the middle. It was probably cut short \
                 in transfer; fetch it again."
            ),
            ZipError::Corrupt { name } => write!(
                f,
                "{name} inside this archive does not match its checksum. \
                 The archive is damaged; fetch it again."
            ),
            ZipError::Compressed { name, method } => write!(
                f,
                "{name} inside this archive is compressed (method {method}), and \
                 this reader takes stored entries only. Unpack the archive with \
                 `unzip` and import the directory instead."
            ),
        }
    }
}

impl std::error::Error for ZipError {}

/// Find the end of central directory record, scanning back from the end.
///
/// Back rather than forward because a zip is defined from its tail: anything
/// may be prepended to one (a self extracting stub, a JAR's shebang) and the
/// directory still says where the members are.
fn find_end(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    // 22 is the fixed part; the comment that may follow is at most 64 KiB.
    let earliest = bytes.len().saturating_sub(22 + u16::MAX as usize);
    (earliest..=bytes.len() - 22)
        .rev()
        .find(|&i| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) == END)
}

/// CRC-32, the one zip uses. The table is built on first use rather than
/// written out as 256 constants nobody can check by eye.
pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: LazyLock<[u32; 256]> = LazyLock::new(|| {
        let mut table = [0u32; 256];
        for (n, slot) in table.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *slot = c;
        }
        table
    });
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc = TABLE[((crc ^ *byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_written_is_what_comes_back() {
        let mut zip = Zip::new();
        assert!(zip.add("collection.json", b"{\"name\":\"Sunday\"}"));
        assert!(zip.add("assets/logo.png", &[0x89, b'P', b'N', b'G', 0, 1, 2, 3]));
        let bytes = zip.finish();
        let back = read(&bytes).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back["collection.json"], b"{\"name\":\"Sunday\"}");
        assert_eq!(back["assets/logo.png"], vec![0x89, b'P', b'N', b'G', 0, 1, 2, 3]);
    }

    #[test]
    fn the_archive_starts_with_the_signature_every_tool_looks_for() {
        let mut zip = Zip::new();
        zip.add("a.txt", b"a");
        let bytes = zip.finish();
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        assert!(bytes.windows(4).any(|w| w == b"PK\x05\x06"));
    }

    #[test]
    fn a_duplicate_name_is_refused_and_the_first_one_stands() {
        let mut zip = Zip::new();
        assert!(zip.add("a.txt", b"first"));
        assert!(!zip.add("a.txt", b"second"));
        let back = read(&zip.finish()).unwrap();
        assert_eq!(back["a.txt"], b"first");
    }

    #[test]
    fn an_empty_member_round_trips() {
        let mut zip = Zip::new();
        zip.add("empty", b"");
        let back = read(&zip.finish()).unwrap();
        assert_eq!(back["empty"], Vec::<u8>::new());
    }

    #[test]
    fn a_file_that_is_not_a_zip_says_so_rather_than_panicking() {
        assert_eq!(read(b"not a zip at all").unwrap_err(), ZipError::NotAZip);
        assert_eq!(read(b"").unwrap_err(), ZipError::NotAZip);
        assert!(read(b"not a zip at all").unwrap_err().to_string().contains("not a zip"));
    }

    #[test]
    fn a_damaged_member_names_the_file_inside_the_archive() {
        let mut zip = Zip::new();
        zip.add("collection.json", b"{}");
        let mut bytes = zip.finish();
        // The payload sits after the 30 byte local header and the name.
        let at = 30 + "collection.json".len();
        bytes[at] = b'X';
        let err = read(&bytes).unwrap_err();
        assert_eq!(err, ZipError::Corrupt { name: "collection.json".into() });
        assert!(err.to_string().contains("collection.json"), "{err}");
    }

    #[test]
    fn the_crc_matches_the_value_the_format_defines() {
        // The check value every CRC-32 implementation is tested against.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
