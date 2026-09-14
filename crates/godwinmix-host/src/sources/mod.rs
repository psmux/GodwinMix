//! Where a plugin comes from, and how it gets here.
//!
//! 06 section 2 lists seven source forms and the rule behind all of them is
//! "reuse existing distribution": a plugin author who already publishes on
//! crates.io, npm or PyPI should not have to publish anywhere else, and a
//! plugin that is only a tagged GitHub release should still install in one
//! command. So there is no GodwinMix package format. There is a directory with
//! `gmx-plugin.toml` at its root, and seven ways of ending up holding one.
//!
//! ```text
//!   owner/repo             github.rs   release assets per platform, signature beside each
//!   https://...git         git.rs      clone, then [build] if the manifest has one
//!   cargo:name             registry.rs cargo install --root into the staged directory
//!   npm:@scope/name        registry.rs npm ci --omit=dev
//!   pypi:name              registry.rs uv venv, or python -m venv, then install
//!   oci:ghcr.io/x/y:1.0    oci.rs      refused, with the next step
//!   ./path                 path.rs     development; unreviewed by definition
//! ```
//!
//! Every one of them ends the same way: a directory that [`crate::verify`] has
//! had its say about, which the loader copies to
//! `<plugins_dir>/<name>/<version>/`. Nothing here knows what a plugin does or
//! how it is launched.

pub mod archive;
pub mod git;
pub mod github;
pub mod http;
pub mod oci;
pub mod path;
pub mod registry;

use crate::verify::{Identity, Trust};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// One of the seven forms, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A directory on this machine. Development, and the only form that can be
    /// installed unsigned without a config switch.
    Path(PathBuf),
    /// `owner/repo`, optionally `@version`. Release assets, one per platform.
    GitHub { owner: String, repo: String, version: Option<String> },
    /// Any git URL, optionally `#branch-or-tag`.
    Git { url: String, reference: Option<String> },
    /// `cargo:name`, optionally `@version`.
    Cargo { package: String, version: Option<String> },
    /// `npm:@scope/name`, optionally `@version`.
    Npm { package: String, version: Option<String> },
    /// `pypi:name`, optionally `@version`.
    PyPi { package: String, version: Option<String> },
    /// `oci:ghcr.io/x/y:1.0`.
    Oci { reference: String },
}

impl Source {
    /// Read a source off the command line or out of a marketplace entry.
    ///
    /// The ambiguity worth knowing about: `owner/repo` and `./owner/repo` are
    /// told apart by the leading dot, and a Windows path (`C:\plugins\clock`)
    /// is told from a scheme by the length of what comes before the colon. A
    /// bare word with no slash is neither, and the error says so rather than
    /// guessing.
    pub fn parse(spec: &str) -> Result<Self> {
        let spec = spec.trim();
        anyhow::ensure!(!spec.is_empty(), "no source was given. `gmx plugin add --help` lists the forms.");
        if let Some(rest) = spec.strip_prefix("cargo:") {
            let (package, version) = split_version(rest);
            return Ok(Source::Cargo { package, version });
        }
        if let Some(rest) = spec.strip_prefix("npm:") {
            let (package, version) = split_version(rest);
            return Ok(Source::Npm { package, version });
        }
        if let Some(rest) = spec.strip_prefix("pypi:") {
            let (package, version) = split_version(rest);
            return Ok(Source::PyPi { package, version });
        }
        if let Some(rest) = spec.strip_prefix("oci:") {
            return Ok(Source::Oci { reference: rest.to_string() });
        }
        if looks_like_a_path(spec) {
            return Ok(Source::Path(PathBuf::from(spec)));
        }
        if let Some(rest) = spec.strip_prefix("git+") {
            let (url, reference) = split_fragment(rest);
            return Ok(Source::Git { url, reference });
        }
        if spec.starts_with("http://") || spec.starts_with("https://") || spec.starts_with("ssh://")
        {
            let (url, reference) = split_fragment(spec);
            if url.ends_with(".git") {
                return Ok(Source::Git { url, reference });
            }
            // A GitHub web URL is what people paste. Turn it into owner/repo.
            if let Some(found) = github::from_url(&url) {
                return Ok(found);
            }
            anyhow::bail!(
                "`{spec}` is a URL but not one this understands. A git source ends in .git; \
                 a GitHub repository is written `owner/repo`."
            );
        }
        if spec.starts_with("git@") {
            let (url, reference) = split_fragment(spec);
            return Ok(Source::Git { url, reference });
        }
        let (name, version) = split_version(spec);
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Ok(Source::GitHub {
                owner: parts[0].to_string(),
                repo: parts[1].to_string(),
                version,
            });
        }
        anyhow::bail!(
            "`{spec}` is not a source. Write one of:\n  \
             owner/repo                a GitHub release\n  \
             https://host/x/y.git      a git repository\n  \
             cargo:gmx-ndi             a crate\n  \
             npm:@scope/gmx-chat       an npm package\n  \
             pypi:gmx-director         a PyPI package\n  \
             oci:ghcr.io/x/y:1.0       a container image\n  \
             ./my-plugin               a directory you are working in\n\
             A bare name with no slash is looked up in your marketplaces; add one with \
             `gmx marketplace add owner/repo`."
        )
    }

    /// What this reads as on a report line and in the trust record.
    pub fn display(&self) -> String {
        match self {
            Source::Path(p) => p.display().to_string(),
            Source::GitHub { owner, repo, version } => match version {
                Some(v) => format!("{owner}/{repo}@{v}"),
                None => format!("{owner}/{repo}"),
            },
            Source::Git { url, reference } => match reference {
                Some(r) => format!("{url}#{r}"),
                None => url.clone(),
            },
            Source::Cargo { package, version } => with_version("cargo", package, version),
            Source::Npm { package, version } => with_version("npm", package, version),
            Source::PyPi { package, version } => with_version("pypi", package, version),
            Source::Oci { reference } => format!("oci:{reference}"),
        }
    }

    /// Does getting this need the network?
    pub fn needs_network(&self) -> bool {
        !matches!(self, Source::Path(_))
    }
}

fn with_version(scheme: &str, package: &str, version: &Option<String>) -> String {
    match version {
        Some(v) => format!("{scheme}:{package}@{v}"),
        None => format!("{scheme}:{package}"),
    }
}

/// `name@1.2.3` into its two halves. A leading `@` is a npm scope, not a
/// version, so the split looks for the last `@` past the first character.
fn split_version(spec: &str) -> (String, Option<String>) {
    match spec.char_indices().skip(1).filter(|(_, c)| *c == '@').last() {
        Some((at, _)) => (spec[..at].to_string(), Some(spec[at + 1..].to_string())),
        None => (spec.to_string(), None),
    }
}

fn split_fragment(spec: &str) -> (String, Option<String>) {
    match spec.split_once('#') {
        Some((url, reference)) => (url.to_string(), Some(reference.to_string())),
        None => (spec.to_string(), None),
    }
}

fn looks_like_a_path(spec: &str) -> bool {
    spec == "."
        || spec == ".."
        || spec.starts_with("./")
        || spec.starts_with("../")
        || spec.starts_with(".\\")
        || spec.starts_with("..\\")
        || spec.starts_with('/')
        || spec.starts_with('~')
        // C:\plugins\clock. One letter before the colon is a drive, not a scheme.
        || (spec.len() > 2
            && spec.as_bytes()[1] == b':'
            && spec.as_bytes()[0].is_ascii_alphabetic()
            && matches!(spec.as_bytes()[2], b'\\' | b'/'))
}

// ---------------------------------------------------------------------------
// Fetching
// ---------------------------------------------------------------------------

/// What a fetch is allowed to do and where it may write.
#[derive(Debug, Clone)]
pub struct FetchCtx {
    /// A scratch directory the caller owns and removes. Everything a fetch
    /// downloads, clones or builds happens under here.
    pub staging: PathBuf,
    /// The platform triple assets are chosen for, from `launch::this_platform`.
    pub platform: String,
    /// Where GitHub's API is. A test points this at a local server; nothing
    /// else ever changes it.
    pub github_api: String,
    /// Refuse anything that would touch the network.
    pub offline: bool,
    /// Who a signature has to be from. `None` means the signature is read but
    /// the identity is not pinned, which is what an unofficial marketplace
    /// gets.
    pub identity: Option<Identity>,
}

impl FetchCtx {
    pub fn new(staging: PathBuf, platform: impl Into<String>) -> Self {
        Self {
            staging,
            platform: platform.into(),
            github_api: default_github_api(),
            offline: false,
            identity: None,
        }
    }
}

/// `https://api.github.com` unless `GMX_GITHUB_API` says otherwise.
pub fn default_github_api() -> String {
    std::env::var("GMX_GITHUB_API").unwrap_or_else(|_| "https://api.github.com".into())
}

/// A plugin directory, and what is known about where it came from.
#[derive(Debug, Clone)]
pub struct Fetched {
    /// The directory with `gmx-plugin.toml` at its root.
    pub dir: PathBuf,
    pub trust: Trust,
    /// Lines worth printing: the asset that was chosen, the build that ran.
    pub notes: Vec<String>,
}

/// Get a source onto this machine.
///
/// The directory it answers with is inside `ctx.staging` for every form but a
/// path, which is used where it lies.
pub fn fetch(source: &Source, ctx: &FetchCtx) -> Result<Fetched> {
    if ctx.offline && source.needs_network() {
        anyhow::bail!(
            "`{}` needs the network and this core is offline. Install it from a directory \
             with `gmx plugin add ./<dir>`, or run without --offline.",
            source.display()
        );
    }
    std::fs::create_dir_all(&ctx.staging)
        .with_context(|| format!("making the staging directory {}", ctx.staging.display()))?;
    match source {
        Source::Path(p) => path::fetch(p),
        Source::GitHub { owner, repo, version } => {
            github::fetch(owner, repo, version.as_deref(), ctx)
        }
        Source::Git { url, reference } => git::fetch(url, reference.as_deref(), ctx),
        Source::Cargo { package, version } => {
            registry::fetch_cargo(package, version.as_deref(), ctx)
        }
        Source::Npm { package, version } => registry::fetch_npm(package, version.as_deref(), ctx),
        Source::PyPi { package, version } => registry::fetch_pypi(package, version.as_deref(), ctx),
        Source::Oci { reference } => oci::refuse(reference),
    }
}

/// Find the directory with `gmx-plugin.toml` in it, at or just under `root`.
///
/// An archive made with `tar czf` from a checkout has one directory at the top
/// and the manifest inside it; one made from inside the checkout has the
/// manifest at the top. Both are common, so both work, and nothing deeper than
/// two levels is searched because that is where a plugin stops being obvious.
pub fn find_manifest_root(root: &Path) -> Result<PathBuf> {
    if root.join("gmx-plugin.toml").is_file() {
        return Ok(root.to_path_buf());
    }
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(root)
        .with_context(|| format!("reading {}", root.display()))?
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() && path.join("gmx-plugin.toml").is_file() {
            candidates.push(path);
        }
    }
    candidates.sort();
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => anyhow::bail!(
            "there is no gmx-plugin.toml in what arrived. Every plugin has one at the root \
             of its directory or one level inside the archive. What was there: {}",
            listing(root)
        ),
        _ => anyhow::bail!(
            "what arrived has {} gmx-plugin.toml files in it, so there is no way to tell \
             which plugin it is. A release asset holds exactly one plugin.",
            candidates.len()
        ),
    }
}

fn listing(root: &Path) -> String {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names.truncate(12);
    if names.is_empty() {
        "nothing".into()
    } else {
        names.join(", ")
    }
}

/// Run a command in a directory and fail with its output when it does not
/// succeed. Used by every source that shells out to a package manager.
pub fn run(what: &str, program: &str, args: &[&str], cwd: &Path) -> Result<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| {
            format!(
                "`{program}` is not installed, or not on PATH. {what} needs it. \
                 Install it and run the add again."
            )
        })?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = stderr.lines().rev().take(12).collect();
    anyhow::bail!(
        "{what} failed: {program} {}\n{}",
        args.join(" "),
        tail.into_iter().rev().collect::<Vec<_>>().join("\n")
    )
}

/// Keep the executable bit when a file is moved into place. A plugin binary
/// that arrives without it starts with "permission denied" and nothing saying
/// why.
#[cfg(unix)]
pub fn copy_mode(from: &Path, to: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(from) {
        let mode = meta.permissions().mode();
        let _ = std::fs::set_permissions(to, std::fs::Permissions::from_mode(mode));
    }
}

#[cfg(not(unix))]
pub fn copy_mode(_from: &Path, _to: &Path) {}

/// Is a program on PATH? Used to choose `uv` over `python -m venv` and to tell
/// an operator whether a container runtime is there.
pub fn have(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests;
