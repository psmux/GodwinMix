//! The gallery's side of the marketplaces: the community index as a default,
//! a copy kept a few minutes, and whether a listing runs on this machine.
//!
//! Nothing here runs unless somebody asks. Opening the gallery fetches a
//! marketplace whose copy is older than [`TTL_SECS`]; searching fetches only
//! one that was never fetched. A marketplace that cannot be reached keeps
//! the copy it had, so the gallery works on a show network with no route out.
//!
//! The default is the community index, `psmux/godwinmix-plugins`. It is added
//! the first time it is wanted, removed like any other with
//! `gmx marketplace remove`, and once removed it stays removed.
//! `GMX_DEFAULT_MARKETPLACE` names another, or turns the default off when
//! set to nothing. `[marketplaces] only` is obeyed: a pinned list that does
//! not name the default never fetches it.

use super::{add_within, load_store, Added, Listing};
use std::sync::Mutex;
use std::time::Duration;

/// The platform word for a release with one asset for every computer: a
/// pure Python plugin, say, whose asset holds source and no binary.
pub const ANY: &str = "any";
pub const DEFAULT_SOURCE: &str = "psmux/godwinmix-plugins";
pub const DEFAULT_NAME: &str = "godwinmix-plugins";
pub const DEFAULT_ENV: &str = "GMX_DEFAULT_MARKETPLACE";
/// How old a copy may be before opening the gallery fetches it again.
pub const TTL_SECS: u64 = 600;
/// How long one request may take while a person looks at the page.
const ASK_FOR: Duration = Duration::from_secs(4);
/// How long a marketplace that could not be reached is left alone.
const RETRY_SECS: u64 = 60;

/// The default marketplace, or none when `GMX_DEFAULT_MARKETPLACE` is empty.
pub fn default_source() -> Option<String> {
    match std::env::var(DEFAULT_ENV) {
        Ok(set) => Some(set.trim().to_string()).filter(|s| !s.is_empty()),
        Err(_) => Some(DEFAULT_SOURCE.to_string()),
    }
}

/// Whether this added marketplace is the default one.
pub fn is_default(added: &Added) -> bool {
    added.name == DEFAULT_NAME || default_source().is_some_and(|d| d == added.source)
}

/// One marketplace after the gallery asked for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Freshness {
    pub name: String,
    pub source: String,
    /// When the copy now on disk was fetched, seconds since the epoch. 0 when
    /// there is none.
    pub fetched: u64,
    /// Why it was not fetched just now, when it was due and was not.
    pub problem: Option<String>,
}

/// How old a copy may be. `None` fetches only what was never fetched.
pub type Ttl = Option<u64>;

/// Bring every marketplace the gallery reads up to date, adding the default
/// one when it is missing. `fetch` is [`super::add`] in a mixer and a stand
/// in under test.
pub fn refresh(
    only: &[String],
    ttl: Ttl,
    now: u64,
    fetch: &dyn Fn(&str) -> anyhow::Result<Added>,
) -> Vec<Freshness> {
    let store = load_store();
    let allowed = |name: &str, source: &str| {
        only.is_empty() || only.iter().any(|o| o == name || o == source)
    };
    let mut wanted: Vec<(String, String, u64)> = store
        .marketplaces
        .iter()
        .filter(|m| allowed(&m.name, &m.source))
        .map(|m| (m.name.clone(), m.source.clone(), m.fetched))
        .collect();
    if let Some(default) = default_source() {
        let have = store.marketplaces.iter().any(|m| m.name == DEFAULT_NAME || m.source == default);
        let declined = store.declined.iter().any(|d| d == DEFAULT_NAME || d == &default);
        if !have && !declined && allowed(DEFAULT_NAME, &default) {
            wanted.push((DEFAULT_NAME.to_string(), default, 0));
        }
    }
    wanted
        .into_iter()
        .map(|(name, source, fetched)| {
            let due = fetched == 0 || ttl.is_some_and(|t| now.saturating_sub(fetched) >= t);
            if !due {
                return Freshness { name, source, fetched, problem: None };
            }
            match fetch(&source) {
                Ok(added) => Freshness { name: added.name, source, fetched: added.fetched, problem: None },
                Err(e) => Freshness { name, source, fetched, problem: Some(format!("{e:#}")) },
            }
        })
        .collect()
}

/// [`refresh`] against the network, as the gallery and a search call it. A
/// marketplace that could not be reached is not asked again for a minute, so
/// a mixer with no route out does not wait on every page that opens.
pub fn refresh_now(only: &[String], ttl: Ttl) -> Vec<Freshness> {
    static FAILED: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());
    let now = super::now();
    refresh(only, ttl, now, &|source| {
        let mut failed = FAILED.lock().unwrap_or_else(|e| e.into_inner());
        failed.retain(|(_, at)| now.saturating_sub(*at) < RETRY_SECS);
        if failed.iter().any(|(s, _)| s == source) {
            anyhow::bail!("it could not be reached a moment ago, so the copy from before is used");
        }
        drop(failed);
        let answer = add_within(source, ASK_FOR);
        if answer.is_err() {
            FAILED.lock().unwrap_or_else(|e| e.into_inner()).push((source.to_string(), now));
        }
        answer
    })
}

/// Whether a listing runs here, as a label a person reads on a card and the
/// sentence behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    pub runs: bool,
    pub label: String,
    pub detail: String,
}

/// Whether `listing` runs on `platform` with this core's api level. `shipped`
/// says whether this copy of the mixer carries the plugin.
pub fn fit(listing: &Listing, platform: &str, shipped: bool) -> Fit {
    let yes = |detail: &str| Fit { runs: true, label: "Runs here".into(), detail: detail.into() };
    let no = |label: String, detail: String| Fit { runs: false, label, detail };
    if listing.display.bundled {
        return if shipped {
            yes("It comes with GodwinMix, so it installs with no download.")
        } else {
            no("Not in this copy".into(), "It comes with GodwinMix, but this copy of the mixer was built without it.".into())
        };
    }
    if newer_than_this(&listing.display.mixer) {
        return no(
            "Needs a newer GodwinMix".into(),
            format!("It needs GodwinMix {} or newer. Update GodwinMix first.", listing.display.mixer),
        );
    }
    if listing.versions.is_empty() {
        return yes("The listing names no versions, so the install checks the rest.");
    }
    let Some(version) = listing.usable_version() else {
        let newest = listing.versions.iter().map(|v| v.api).max().unwrap_or(0);
        return if newest > godwinmix_protocol::API_LEVEL {
            no("Needs a newer GodwinMix".into(), format!("It needs protocol level {newest}. Update GodwinMix first."))
        } else {
            no("Made for an older GodwinMix".into(), format!("It was made for protocol level {newest}, which this mixer no longer runs."))
        };
    };
    if version.platforms.is_empty() || version.platforms.iter().any(|p| p == platform) {
        return yes("There is a build of it for this computer.");
    }
    if version.platforms.iter().any(|p| p == ANY) {
        return yes("One download runs on every computer, so it installs here.");
    }
    let system = os_name(platform);
    let same_os = version.platforms.iter().any(|p| os_name(p) == system);
    let label = if same_os { "Not for this computer".to_string() } else { format!("Not for {system}") };
    no(label, format!("It is built for {} only.", version.platforms.join(", ")))
}

/// Whether a version a listing asks for is newer than this GodwinMix.
pub fn newer_than_this(wanted: &str) -> bool {
    newer_than(wanted, env!("CARGO_PKG_VERSION"))
}

/// Whether version `a` is newer than `b`, number by number, ignoring
/// anything after a `-` or a `+`. An empty `a` is never newer.
pub fn newer_than(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        let v = v.trim().trim_start_matches('v');
        v.split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    if a.trim().is_empty() {
        return false;
    }
    let (a, b) = (parts(a), parts(b));
    (0..a.len().max(b.len()))
        .map(|i| (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0)))
        .find(|(x, y)| x != y)
        .is_some_and(|(x, y)| x > y)
}

/// "Windows" for `windows-x86_64`: the word on the card.
pub fn os_name(platform: &str) -> &'static str {
    match platform.split('-').next().unwrap_or("") {
        "windows" => "Windows",
        "macos" => "Mac",
        "linux" => "Linux",
        _ => "this computer",
    }
}

#[cfg(test)]
#[path = "gallery_tests.rs"]
mod tests;
