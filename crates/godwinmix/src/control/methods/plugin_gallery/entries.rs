//! Every listing in every marketplace, as gallery cards, with what this
//! machine has installed and whether each one runs here. Pure: the handler
//! hands it the documents and the facts about this machine.

use super::types::{GalleryEntry, GalleryMedia};
use godwinmix_host::marketplace::{Display, Media};
use godwinmix_host::marketplace::gallery::fit;
use godwinmix_host::marketplace::{Listing, Marketplace};
use std::cmp::Reverse;
use std::collections::HashSet;

/// What the gallery needs to know about this machine.
pub struct Here<'a> {
    /// `windows-x86_64`.
    pub platform: &'a str,
    /// Every installed plugin, as name and version.
    pub installed: &'a [(String, String)],
    /// Whether this copy of the mixer carries a plugin of this name.
    pub shipped: &'a dyn Fn(&str) -> bool,
}

/// One card per plugin name. A name listed twice is taken from the higher
/// tier, and from the marketplace added first when tiers match, which is the
/// listing a bare `plugin.add <name>` resolves to.
pub fn entries(docs: &[Marketplace], here: &Here) -> Vec<GalleryEntry> {
    let mut all: Vec<(&Marketplace, &Listing)> =
        docs.iter().flat_map(|m| m.plugins.iter().map(move |l| (m, l))).collect();
    all.sort_by_key(|(_, l)| Reverse(l.tier));
    let mut seen = HashSet::new();
    all.retain(|(_, l)| seen.insert(l.name.clone()));
    all.sort_by(|(_, a), (_, b)| {
        b.tier
            .cmp(&a.tier)
            .then(b.display.stars.unwrap_or(0).cmp(&a.display.stars.unwrap_or(0)))
            .then(a.name.cmp(&b.name))
    });
    all.into_iter().map(|(m, l)| entry(m, l, here)).collect()
}

fn entry(market: &Marketplace, listing: &Listing, here: &Here) -> GalleryEntry {
    let d = &listing.display;
    let usable = listing.usable_version().or(listing.versions.last());
    let version = usable.map(|v| v.version.clone()).unwrap_or_default();
    let installed_version =
        here.installed.iter().find(|(n, _)| n == &listing.name).map(|(_, v)| v.clone());
    let fits = fit(listing, here.platform, (here.shipped)(&listing.name));
    GalleryEntry {
        name: listing.name.clone(),
        title: if d.title.trim().is_empty() { listing.name.clone() } else { d.title.clone() },
        description: listing.description.clone(),
        tier: listing.tier.to_string(),
        kinds: listing.kinds.clone(),
        license: listing.license.clone(),
        repository: listing.repository.clone(),
        homepage: d.homepage.clone(),
        author: if d.author.is_empty() { market.owner.clone() } else { d.author.clone() },
        icon: d.icon.clone(),
        thumbnail: d.thumbnail.clone(),
        screenshots: d.screenshots.clone(),
        media: media_of(d),
        preview: preview_of(d),
        featured: d.featured,
        stars: d.stars,
        downloads: d.downloads,
        updated: d.updated.clone(),
        tags: d.tags.clone(),
        bundled: d.bundled,
        update: installed_version.as_deref().is_some_and(|have| newer(&version, have)),
        installed: installed_version.is_some(),
        installed_version,
        versions: listing.versions.iter().map(|v| v.version.clone()).collect(),
        signed: usable.is_some_and(|v| v.signed),
        version,
        install: listing.name.clone(),
        marketplace: market.name.clone(),
        runs_here: fits.runs,
        fit: fits.label,
        fit_detail: fits.detail,
    }
}

/// The carousel: the index's `media`, or its `screenshots` read by extension
/// for an index written before `media` existed. A kind this mixer does not
/// know is shown as a picture.
pub fn media_of(d: &Display) -> Vec<GalleryMedia> {
    let listed: Vec<Media> = if d.media.is_empty() {
        d.screenshots.iter().map(|u| Media::guess(u)).collect()
    } else {
        d.media.clone()
    };
    listed
        .into_iter()
        .filter(|m| !m.url.trim().is_empty())
        .map(|m| {
            let known = matches!(m.kind.as_str(), "image" | "animation" | "video");
            GalleryMedia { kind: if known { m.kind } else { "image".into() }, url: m.url }
        })
        .collect()
}

/// What a card plays on hover: the index's `preview`, else the first thing
/// in the carousel that moves.
pub fn preview_of(d: &Display) -> String {
    if !d.preview.is_empty() {
        return d.preview.clone();
    }
    media_of(d).into_iter().find(|m| m.kind != "image").map(|m| m.url).unwrap_or_default()
}

/// Whether version `a` is newer than `b`, comparing the numbers in each part.
/// Anything after a `-` or a `+` is ignored, which is close enough to semver
/// for an Update button.
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.split(['-', '+'])
            .next()
            .unwrap_or("")
            .trim_start_matches('v')
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parts(a), parts(b));
    (0..a.len().max(b.len()))
        .map(|i| (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0)))
        .find(|(x, y)| x != y)
        .is_some_and(|(x, y)| x > y)
}
