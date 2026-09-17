//! Marketplaces: a JSON file in a repository that says what plugins exist and
//! where each one comes from.
//!
//! Claude Code's idea, kept whole. A marketplace is not a service and not a
//! database; it is `godwinmix-marketplace.json` in any repository anybody can
//! host. The project runs one official marketplace (the first party plugins)
//! and one community one (the `godwinmix-plugins` index, whose `index.json`
//! has the same shape plus the harness results the bot writes). An
//! organisation pins its operators to a list with `[marketplaces] only` and
//! nothing else is consulted.
//!
//! ```text
//!   gmx marketplace add psmux/godwinmix-plugins
//!     -> https://raw.githubusercontent.com/psmux/godwinmix-plugins/HEAD/index.json
//!     -> cached at ~/.godwinmix/marketplaces/godwinmix-plugins.json
//!
//!   gmx plugin search ndi        reads every cached file
//!   gmx plugin add ndi           resolves the name to psmux/gmx-ndi and installs that
//! ```
//!
//! The cache is a copy, never the truth: `gmx marketplace add` and
//! `gmx plugin search --refresh` fetch, and everything else reads what is
//! already there so that a search costs nothing and works on a show network
//! with no route out.

use crate::sources::{http, Source};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The two file names a marketplace repository may use. `index.json` is what
/// the community index carries because it holds more than a listing; both are
/// read with the same parser.
pub const FILE_NAMES: &[&str] = &["godwinmix-marketplace.json", "index.json"];

/// One marketplace document.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Marketplace {
    /// A slug. It names the cache file and is what `gmx marketplace remove`
    /// takes.
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Who runs it, for the listing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub owner: String,
    /// The schema version of this document. 1 is what this core reads.
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub plugins: Vec<Listing>,
    /// Who signs the assets this marketplace lists, when its CI signs them.
    /// A plugin resolved through a marketplace with this set is verified
    /// against that identity rather than against nobody in particular.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing: Option<Signing>,
}

/// The sigstore identity a marketplace's CI signs with.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Signing {
    /// A regular expression the certificate's subject must match.
    pub identity_regexp: String,
    /// The OIDC issuer, `https://token.actions.githubusercontent.com` for a
    /// marketplace whose CI is GitHub Actions.
    pub oidc_issuer: String,
}

fn one() -> u32 {
    1
}

/// One plugin in a marketplace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Listing {
    /// The plugin's own name, which is the namespace of every id it
    /// contributes. What `gmx plugin add <name>` matches.
    pub name: String,
    /// Where to get it, in any of the forms [`Source`] parses.
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The quality tier off 06 section 4.
    #[serde(default)]
    pub tier: Tier,
    /// The versions listed, newest last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<Version>,
    /// The kinds it provides: source, output, filter, panel, preset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository: String,
    /// Anything the index CI adds that this core does not read.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// One published version of a listed plugin.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Version {
    pub version: String,
    /// The protocol level this version declares.
    pub api: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    /// Whether the index CI signed the assets for this version.
    #[serde(default)]
    pub signed: bool,
    /// The harness result the bot recorded, per platform.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub harness: Vec<String>,
}

/// 06 section 4, read off every catalogue entry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Listed by topic or added by path. Nothing was checked.
    #[default]
    Custom,
    Bronze,
    Silver,
    Gold,
}

impl Tier {
    pub fn label(&self) -> &'static str {
        match self {
            Tier::Custom => "custom, unreviewed",
            Tier::Bronze => "bronze",
            Tier::Silver => "silver",
            Tier::Gold => "gold",
        }
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Tier::Custom => "custom",
            Tier::Bronze => "bronze",
            Tier::Silver => "silver",
            Tier::Gold => "gold",
        })
    }
}

impl Marketplace {
    pub fn parse(text: &str) -> Result<Self> {
        let parsed: Marketplace = serde_json::from_str(text)
            .context("this is not a marketplace document. It needs `name` and `plugins`.")?;
        parsed.check()?;
        Ok(parsed)
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    fn check(&self) -> Result<()> {
        anyhow::ensure!(
            !self.name.trim().is_empty(),
            "a marketplace needs a `name`: a slug that names it in `gmx marketplace list`."
        );
        anyhow::ensure!(
            self.version <= 1,
            "this marketplace declares schema version {}, and this core reads version 1. \
             Upgrade GodwinMix, or ask whoever runs it for a version 1 document.",
            self.version
        );
        for listing in &self.plugins {
            anyhow::ensure!(
                !listing.name.trim().is_empty(),
                "a plugin in `{}` has no name.",
                self.name
            );
            Source::parse(&listing.source).with_context(|| {
                format!("the source of `{}` in marketplace `{}`", listing.name, self.name)
            })?;
        }
        Ok(())
    }

    pub fn find(&self, name: &str) -> Option<&Listing> {
        self.plugins.iter().find(|p| p.name == name)
    }

    /// What a signature from this marketplace has to be, if it says.
    pub fn signing_identity(&self) -> Option<crate::verify::Identity> {
        self.signing.as_ref().map(|s| crate::verify::Identity {
            identity_regexp: s.identity_regexp.clone(),
            oidc_issuer: s.oidc_issuer.clone(),
        })
    }
}

impl Listing {
    pub fn source(&self) -> Result<Source> {
        Source::parse(&self.source)
    }

    /// The newest listed version this core's api range can run, if any.
    pub fn usable_version(&self) -> Option<&Version> {
        let compatible = godwinmix_protocol::API_COMPATIBLE;
        let level = godwinmix_protocol::API_LEVEL;
        self.versions
            .iter()
            .rev()
            .find(|v| (compatible..=level).contains(&v.api))
    }
}

// ---------------------------------------------------------------------------
// The operator's list
// ---------------------------------------------------------------------------

/// One marketplace the operator added.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Added {
    pub name: String,
    /// What was typed: `owner/repo`, a URL or a path.
    pub source: String,
    /// The URL or path the document was last read from.
    #[serde(default)]
    pub url: String,
    /// When it was last fetched, as seconds since the epoch. A number rather
    /// than a date so no date crate is needed to write it.
    #[serde(default)]
    pub fetched: u64,
    #[serde(default)]
    pub plugins: usize,
}

/// The list, as it is stored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub marketplaces: Vec<Added>,
}

/// `~/.godwinmix` unless something says otherwise.
pub fn home_dir() -> PathBuf {
    if let Ok(explicit) = std::env::var("GODWINMIX_HOME") {
        return PathBuf::from(explicit);
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".godwinmix")
}

/// Where the list of added marketplaces lives.
pub fn store_path() -> PathBuf {
    home_dir().join("marketplaces.json")
}

/// Where the cached copy of one marketplace's document lives.
pub fn cache_path(name: &str) -> PathBuf {
    home_dir().join("marketplaces").join(format!("{name}.json"))
}

pub fn load_store() -> Store {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_store(store: &Store) -> Result<()> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("making {}", parent.display()))?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(store)?)
        .with_context(|| format!("writing {}", path.display()))
}

/// Add a marketplace: fetch it, check it, cache it, and record it.
pub fn add(spec: &str) -> Result<Added> {
    let (url, doc) = fetch_document(spec)?;
    let cache = cache_path(&doc.name);
    if let Some(parent) = cache.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("making {}", parent.display()))?;
    }
    std::fs::write(&cache, serde_json::to_string_pretty(&doc)?)
        .with_context(|| format!("caching {}", cache.display()))?;
    let entry = Added {
        name: doc.name.clone(),
        source: spec.to_string(),
        url,
        fetched: now(),
        plugins: doc.plugins.len(),
    };
    let mut store = load_store();
    store.marketplaces.retain(|m| m.name != entry.name);
    store.marketplaces.push(entry.clone());
    store.marketplaces.sort_by(|a, b| a.name.cmp(&b.name));
    save_store(&store)?;
    Ok(entry)
}

/// Fetch every added marketplace again. Answers what changed.
pub fn refresh() -> Vec<(String, Result<usize>)> {
    let store = load_store();
    let mut out = Vec::new();
    for entry in store.marketplaces {
        let result = add(&entry.source).map(|a| a.plugins);
        out.push((entry.name, result));
    }
    out
}

pub fn remove(name: &str) -> Result<Added> {
    let mut store = load_store();
    let found = store
        .marketplaces
        .iter()
        .position(|m| m.name == name)
        .with_context(|| {
            let have: Vec<&str> =
                store.marketplaces.iter().map(|m| m.name.as_str()).collect();
            format!(
                "there is no marketplace called `{name}`. Added: {}.",
                if have.is_empty() { "none".into() } else { have.join(", ") }
            )
        })?;
    let gone = store.marketplaces.remove(found);
    save_store(&store)?;
    let _ = std::fs::remove_file(cache_path(name));
    Ok(gone)
}

/// A marketplace the project runs, offered to a machine that has none.
///
/// A fresh core knows no marketplaces at all, so `plugin.search` answers
/// nothing and `plugin.add camera` cannot resolve a bare name. The CLI has
/// always printed these two as the next thing to type; a surface with no
/// terminal needs the same two as data it can put behind a button.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Recommendation {
    /// The slug the document carries, which is what it will be listed under.
    pub name: String,
    /// What to pass to `marketplace.add`.
    pub source: String,
    pub title: String,
    pub description: String,
    /// Whether this machine has it already.
    pub added: bool,
    /// True for the one the project itself publishes.
    pub first_party: bool,
}

/// The two the project runs, first party first.
const RECOMMENDED: &[(&str, &str, &str, &str, bool)] = &[
    (
        "godwinmix",
        "psmux/godwinmix",
        "GodwinMix official",
        "The plugins the project maintains: the camera, the audio device, NDI, SRT and \
         the rest. Built on the public sidecar contract, run through the conformance \
         harness in CI on every platform they declare, and signed by the release workflow.",
        true,
    ),
    (
        "godwinmix-plugins",
        "psmux/godwinmix-plugins",
        "Community index",
        "Everything anybody has published and the harness bot has checked, with the \
         result of that check beside each version. Most of it is nobody's responsibility \
         but its author's, so read the tier before installing.",
        false,
    ),
];

/// What to offer a machine, narrowed by `only` the way a search is.
///
/// An operator who pinned `[marketplaces] only` gets nothing offered that the
/// pin would refuse to read: a button that adds a marketplace this core then
/// ignores is worse than no button.
pub fn recommendations(only: &[String]) -> Vec<Recommendation> {
    let have = load_store();
    RECOMMENDED
        .iter()
        .filter(|(name, source, ..)| {
            only.is_empty() || only.iter().any(|o| o == name || o == source)
        })
        .map(|(name, source, title, description, first_party)| Recommendation {
            name: (*name).to_string(),
            source: (*source).to_string(),
            title: (*title).to_string(),
            description: description.split_whitespace().collect::<Vec<_>>().join(" "),
            added: have.marketplaces.iter().any(|m| &m.name == name || &m.source == source),
            first_party: *first_party,
        })
        .collect()
}

/// Whether `only` would ever read a marketplace under this name and spec.
///
/// Read before an add rather than after: adding one that the pin excludes
/// writes a line nothing consults, and the operator finds out when a search
/// comes back empty.
pub fn pinned_out(name: &str, spec: &str, only: &[String]) -> bool {
    !only.is_empty() && !only.iter().any(|o| o == name || o == spec)
}

/// Every marketplace document this machine has, in the order they were added,
/// narrowed by `only` when the operator pinned a list.
pub fn documents(only: &[String]) -> Vec<Marketplace> {
    load_store()
        .marketplaces
        .iter()
        .filter(|m| only.is_empty() || only.iter().any(|o| o == &m.name || o == &m.source))
        .filter_map(|m| Marketplace::read(&cache_path(&m.name)).ok())
        .collect()
}

/// Find a plugin by name across every marketplace, newest tier first.
///
/// A plugin listed in two marketplaces is taken from the higher tier, and from
/// the one added first when the tiers match: an organisation that pins its own
/// marketplace ahead of the community one gets its own build.
pub fn resolve(name: &str, only: &[String]) -> Option<(Marketplace, Listing)> {
    let mut found: Vec<(Marketplace, Listing)> = documents(only)
        .into_iter()
        .filter_map(|m| m.find(name).cloned().map(|l| (m, l)))
        .collect();
    found.sort_by_key(|found| std::cmp::Reverse(found.1.tier));
    found.into_iter().next()
}

/// Everything whose name or description mentions `term`.
pub fn search(term: &str, only: &[String]) -> Vec<(String, Listing)> {
    let needle = term.trim().to_lowercase();
    let mut hits = Vec::new();
    for market in documents(only) {
        for listing in &market.plugins {
            let matched = needle.is_empty()
                || listing.name.to_lowercase().contains(&needle)
                || listing.description.to_lowercase().contains(&needle)
                || listing.kinds.iter().any(|k| k.to_lowercase() == needle);
            if matched {
                hits.push((market.name.clone(), listing.clone()));
            }
        }
    }
    hits.sort_by(|a, b| b.1.tier.cmp(&a.1.tier).then_with(|| a.1.name.cmp(&b.1.name)));
    hits
}

/// Read a marketplace document from wherever `spec` points.
fn fetch_document(spec: &str) -> Result<(String, Marketplace)> {
    let spec = spec.trim();
    // A local path, which is how the official marketplace in this repository
    // is added and how a test adds one.
    let as_path = Path::new(spec);
    if as_path.exists() {
        let file = if as_path.is_dir() {
            FILE_NAMES
                .iter()
                .map(|n| as_path.join(n))
                .find(|p| p.is_file())
                .with_context(|| {
                    format!(
                        "there is no {} in {}. A marketplace is a repository with one of \
                         those at its root.",
                        FILE_NAMES.join(" or "),
                        as_path.display()
                    )
                })?
        } else {
            as_path.to_path_buf()
        };
        let doc = Marketplace::read(&file)?;
        return Ok((file.display().to_string(), doc));
    }
    let urls = candidate_urls(spec)?;
    let mut last = String::new();
    for url in &urls {
        match http::get_json(url) {
            Ok(value) => {
                let doc = Marketplace::parse(&value.to_string())
                    .with_context(|| format!("reading {url}"))?;
                return Ok((url.clone(), doc));
            }
            Err(e) => last = format!("{e}"),
        }
    }
    anyhow::bail!(
        "no marketplace document was found for `{spec}`. Tried:\n  {}\nThe last answer was: \
         {last}\nA marketplace is a repository with {} at its root; \
         docs/how-to/run-a-marketplace.md says how to make one.",
        urls.join("\n  "),
        FILE_NAMES.join(" or ")
    )
}

/// The URLs to try, in order, for `owner/repo` or a bare URL.
fn candidate_urls(spec: &str) -> Result<Vec<String>> {
    if spec.starts_with("http://") || spec.starts_with("https://") {
        if spec.ends_with(".json") {
            return Ok(vec![spec.to_string()]);
        }
        let base = spec.trim_end_matches('/');
        return Ok(FILE_NAMES.iter().map(|n| format!("{base}/{n}")).collect());
    }
    let parts: Vec<&str> = spec.split('/').collect();
    anyhow::ensure!(
        parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty(),
        "`{spec}` is not a marketplace. Write `owner/repo`, a URL, or a path to a \
         directory with {} in it.",
        FILE_NAMES.join(" or ")
    );
    let raw = std::env::var("GMX_RAW_BASE")
        .unwrap_or_else(|_| "https://raw.githubusercontent.com".into());
    let base = raw.trim_end_matches('/');
    Ok(FILE_NAMES
        .iter()
        .map(|n| format!("{base}/{}/{}/HEAD/{n}", parts[0], parts[1]))
        .collect())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
