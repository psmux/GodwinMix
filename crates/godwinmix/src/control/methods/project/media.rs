//! The clips a project names, and their bytes when a person asks for them.
//!
//! Listed by walking the media folder, not through `media.list`, which opens
//! every file to read its length: an export should not demux a library to
//! learn names and sizes.

use super::bundle::MediaEntry;
use base64::Engine;
use godwinmix_protocol::error::RpcError;
use std::path::Path;

/// Past this, clips are not put inside a project file: copy the folder.
pub const INLINE_LIMIT: u64 = 256 * 1024 * 1024;

/// Every clip under `dir`, by the name `media.list` would give it.
pub fn list(dir: &Path, depth: usize) -> Vec<MediaEntry> {
    let mut out = Vec::new();
    walk(dir, dir, depth, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<MediaEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() && depth > 0 {
            walk(root, &path, depth - 1, out);
        } else if meta.is_file() && godwinmix_core::media::is_media(&path) {
            let Ok(rel) = path.strip_prefix(root) else { continue };
            let name = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            out.push(MediaEntry { name, size: meta.len(), data: None });
        }
    }
}

/// Put each clip's bytes in, or refuse the whole export with the size.
pub fn include(dir: &Path, entries: &mut [MediaEntry]) -> Result<(), RpcError> {
    let total: u64 = entries.iter().map(|e| e.size).sum();
    if total > INLINE_LIMIT {
        return Err(RpcError::invalid_params(format!(
            "the media folder holds {} MB, too much to put inside one project file (the most is {} MB). \
             Save the project without media and copy the folder {} across instead.",
            total / 1_048_576,
            INLINE_LIMIT / 1_048_576,
            dir.display()
        ))
        .with("field", "include_media")
        .with("bytes", total));
    }
    for entry in entries.iter_mut() {
        let bytes = std::fs::read(dir.join(&entry.name))
            .map_err(|e| RpcError::internal(format!("reading the clip {}: {e}", entry.name)))?;
        entry.data = Some(base64::engine::general_purpose::STANDARD.encode(bytes));
    }
    Ok(())
}

/// What an import does with one clip: `add`, `keep` when the same name and
/// size are already here, `missing` when the file named it without bytes,
/// `skip` when a different clip of that name is here.
pub fn action(dir: &Path, entry: &MediaEntry) -> &'static str {
    let here = std::fs::metadata(dir.join(&entry.name)).ok().filter(|m| m.is_file());
    match (here, &entry.data) {
        (Some(m), _) if m.len() == entry.size => "keep",
        (Some(_), _) => "skip",
        (None, Some(_)) => "add",
        (None, None) => "missing",
    }
}

/// Write one clip the file carried. The name is checked as an upload's is:
/// no climbing out of the folder.
pub fn write(dir: &Path, entry: &MediaEntry) -> Result<(), String> {
    let Some(data) = &entry.data else { return Ok(()) };
    if entry.name.split('/').any(|p| p.is_empty() || p == "." || p == ".." || p.contains('\\')) {
        return Err(format!("{:?} is not a name a clip can have here", entry.name));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("the bytes of {} are damaged: {e}", entry.name))?;
    let to = dir.join(&entry.name);
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("making {}: {e}", parent.display()))?;
    }
    std::fs::write(&to, bytes).map_err(|e| format!("writing {}: {e}", to.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_are_listed_by_name_and_size_and_come_back_with_their_bytes() {
        let dir = std::env::temp_dir().join(format!("gmx-project-media-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("ads")).unwrap();
        std::fs::write(dir.join("intro.mp4"), b"abc").unwrap();
        std::fs::write(dir.join("ads/one.png"), b"12345").unwrap();
        std::fs::write(dir.join("notes.txt"), b"not a clip").unwrap();
        let mut found = list(&dir, 2);
        let names: Vec<&str> = found.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["ads/one.png", "intro.mp4"]);
        include(&dir, &mut found).unwrap();
        let other = dir.join("elsewhere");
        assert_eq!(action(&other, &found[0]), "add");
        write(&other, &found[0]).unwrap();
        assert_eq!(std::fs::read(other.join("ads/one.png")).unwrap(), b"12345");
        assert_eq!(action(&other, &found[0]), "keep");
        let evil = MediaEntry { name: "../x.mp4".into(), size: 1, data: Some("YQ==".into()) };
        assert!(write(&other, &evil).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
