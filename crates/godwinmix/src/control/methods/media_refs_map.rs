//! Where a moved file or folder is now, for every way a reference can spell
//! it: an absolute path, a `file:` URI, a path relative to the working
//! folder, a name in the library (`Sermons/logo.png`), and each of those
//! behind `template:`, `html:` or `media:`.

use godwinmix_core::media_org::Moved;
use std::path::{Path, PathBuf};

struct Pair {
    from_rel: String,
    to_rel: String,
    from_abs: String,
    to_abs: PathBuf,
    /// A folder is matched only by what is inside it, never by its bare
    /// name, which could equally be a scene's or a source's.
    dir: bool,
}

pub struct Mapping(Vec<Pair>);

/// A path as one comparable string: absolute, `/` between names, no `\\?\`,
/// and on Windows without case.
fn norm(p: &Path) -> String {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let s = abs.display().to_string().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s).to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// `rest` when `s` is `base` or under it, `None` otherwise.
fn under<'a>(s: &'a str, base: &str, dir: bool) -> Option<&'a str> {
    if s == base && !dir {
        return Some("");
    }
    s.strip_prefix(base).filter(|r| r.starts_with('/'))
}

impl Mapping {
    pub fn new(root: &Path, moved: &[Moved]) -> Self {
        let mut pairs = Vec::new();
        for m in moved {
            let dir = m.to_path.is_dir();
            pairs.push(Pair { from_rel: m.from.clone(), to_rel: m.to.clone(), from_abs: norm(&m.from_path), to_abs: m.to_path.clone(), dir });
            for (a, b) in &m.also {
                let rel = |p: &Path| p.strip_prefix(root).map(|r| r.display().to_string().replace('\\', "/")).unwrap_or_default();
                pairs.push(Pair { from_rel: rel(a), to_rel: rel(b), from_abs: norm(a), to_abs: b.clone(), dir: false });
            }
        }
        Self(pairs)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// A library name's new spelling.
    pub fn rel(&self, s: &str) -> Option<String> {
        self.0.iter().find_map(|p| under(s, &p.from_rel, p.dir).map(|rest| format!("{}{rest}", p.to_rel)))
    }

    /// A path's new spelling, absolute.
    pub fn path(&self, s: &str) -> Option<PathBuf> {
        let n = norm(Path::new(s));
        self.0.iter().find_map(|p| {
            under(&n, &p.from_abs, p.dir).map(|rest| {
                let tail = &s[s.len() - rest.len()..];
                PathBuf::from(format!("{}{}", p.to_abs.display(), tail.replace('/', std::path::MAIN_SEPARATOR_STR)))
            })
        })
    }

    /// A source address, in whatever form it was written. A library name
    /// counts only behind `template:` and `html:`, which read one.
    pub fn address(&self, s: &str) -> Option<String> {
        for prefix in ["template:", "html:"] {
            if let Some(rest) = s.strip_prefix(prefix) {
                return self.rel(rest).or_else(|| self.path(rest).map(|p| p.display().to_string())).map(|n| format!("{prefix}{n}"));
            }
        }
        if s.starts_with("file:") {
            let (path, _) = gstreamer::glib::filename_from_uri(s).ok()?;
            return self.path(&path.display().to_string()).map(|p| godwinmix_core::input::file_uri(&p));
        }
        if s.contains("://") {
            return None;
        }
        self.path(s).map(|p| p.display().to_string())
    }

    /// A value a field or a graphic holds: a library name, a `media:` one,
    /// or any address.
    pub fn value(&self, s: &str) -> Option<String> {
        if let Some(rest) = s.strip_prefix("media:") {
            return self.rel(rest).map(|n| format!("media:{n}"));
        }
        self.rel(s).or_else(|| self.address(s))
    }

    /// Text with every quoted reference rewritten, or `None` when nothing in
    /// it named a moved file: an SVG's `href="logo.png"`, a `graphic.toml`'s
    /// `logo = "Sermons/logo.png"`.
    pub fn text(&self, text: &str) -> Option<String> {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        let mut changed = false;
        while let Some(at) = rest.find(['"', '\'']) {
            let q = rest.as_bytes()[at] as char;
            out.push_str(&rest[..=at]);
            rest = &rest[at + 1..];
            let Some(end) = rest.find(q) else { break };
            let token = &rest[..end];
            match self.value(token) {
                Some(new) if !token.is_empty() => {
                    out.push_str(&new);
                    changed = true;
                }
                _ => out.push_str(token),
            }
            out.push(q);
            rest = &rest[end + 1..];
        }
        out.push_str(rest);
        changed.then_some(out)
    }

    /// Every string in a JSON value rewritten in place. True when any was.
    pub fn json(&self, v: &mut serde_json::Value) -> bool {
        match v {
            serde_json::Value::String(s) => self.value(s).map(|n| *s = n).is_some(),
            serde_json::Value::Array(a) => a.iter_mut().fold(false, |c, x| self.json(x) | c),
            serde_json::Value::Object(o) => o.values_mut().fold(false, |c, x| self.json(x) | c),
            _ => false,
        }
    }
}
