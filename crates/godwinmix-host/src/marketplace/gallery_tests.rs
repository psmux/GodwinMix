use super::*;
use crate::marketplace::tests::{write_market, Home};
use crate::marketplace::{add, documents, load_store, remove, Marketplace, Version};
use std::cell::RefCell;

fn listing(json: serde_json::Value) -> Listing {
    serde_json::from_value(json).expect("a listing")
}

#[test]
fn an_old_listing_parses_with_no_display_fields() {
    let old = listing(serde_json::json!({
        "name": "clock", "source": "psmux/gmx-clock", "tier": "bronze", "badge": "b.json"
    }));
    assert_eq!(old.display, crate::marketplace::Display::default());
    assert!(old.extra.contains_key("badge"));
}

#[test]
fn a_new_listing_carries_its_pictures_stars_and_downloads_and_writes_them_back() {
    let text = serde_json::json!({
        "name": "clock", "source": "psmux/gmx-clock", "tier": "custom", "title": "Clock",
        "icon": "https://x/icon.png", "thumbnail": "https://x/shot.png",
        "screenshots": ["https://x/1.png", "https://x/2.png"], "stars": 12, "downloads": 340,
        "updated": "2026-10-01T09:00:00Z", "author": "psmux", "homepage": "https://x",
        "tags": ["clock"], "bundled": true, "badge": "b.json"
    });
    let new = listing(text.clone());
    assert_eq!(new.display.title, "Clock");
    assert_eq!(new.display.screenshots.len(), 2);
    assert_eq!((new.display.stars, new.display.downloads), (Some(12), Some(340)));
    assert!(new.display.bundled);
    assert_eq!(new.extra.len(), 1, "only the field this core does not read is extra: {:?}", new.extra);
    assert_eq!(serde_json::to_value(&new).expect("it writes"), text);
}

#[test]
fn the_default_marketplace_is_fetched_once_when_missing_and_never_after_it_is_removed() {
    let home = Home::new("gallery-default");
    std::env::remove_var(DEFAULT_ENV);
    let asked = RefCell::new(Vec::new());
    let market = write_market(&home.dir.join("community"), DEFAULT_NAME, serde_json::json!([]));
    let fetch = |source: &str| {
        asked.borrow_mut().push(source.to_string());
        add(market.to_str().expect("a path"))
    };
    let first = refresh(&[], Some(TTL_SECS), crate::marketplace::now(), &fetch);
    assert_eq!(asked.borrow().as_slice(), [DEFAULT_SOURCE]);
    assert_eq!(first[0].name, DEFAULT_NAME);
    assert!(first[0].problem.is_none());
    // Fetched a moment ago: a second open reads the copy.
    refresh(&[], Some(TTL_SECS), crate::marketplace::now(), &fetch);
    assert_eq!(asked.borrow().len(), 1);
    remove(DEFAULT_NAME).expect("it is removed");
    assert!(refresh(&[], Some(TTL_SECS), crate::marketplace::now(), &fetch).is_empty());
    assert_eq!(load_store().declined, vec![DEFAULT_NAME.to_string()]);
}

#[test]
fn a_pinned_list_that_does_not_name_the_default_never_fetches_it() {
    let _home = Home::new("gallery-pinned");
    std::env::remove_var(DEFAULT_ENV);
    let fetch = |_: &str| -> anyhow::Result<Added> { panic!("nothing is fetched") };
    assert!(refresh(&["acme".to_string()], Some(0), 1, &fetch).is_empty());
}

#[test]
fn a_marketplace_that_cannot_be_reached_keeps_its_last_copy() {
    let home = Home::new("gallery-offline");
    std::env::set_var(DEFAULT_ENV, "");
    let market = write_market(&home.dir.join("m"), "local", serde_json::json!([
        { "name": "clock", "source": "psmux/gmx-clock", "stars": 3 }
    ]));
    let added = add(market.to_str().expect("a path")).expect("added");
    let fetch = |_: &str| -> anyhow::Result<Added> { anyhow::bail!("no route to host") };
    let later = added.fetched + TTL_SECS + 1;
    let answer = refresh(&[], Some(TTL_SECS), later, &fetch);
    std::env::remove_var(DEFAULT_ENV);
    assert_eq!(answer.len(), 1);
    assert_eq!(answer[0].fetched, added.fetched);
    assert!(answer[0].problem.as_deref().is_some_and(|p| p.contains("no route")));
    let docs: Vec<Marketplace> = documents(&[]);
    assert_eq!(docs[0].plugins[0].display.stars, Some(3), "the cached copy is still read");
}

fn versioned(api: u32, platforms: &[&str]) -> Listing {
    let mut l = listing(serde_json::json!({ "name": "ndi", "source": "psmux/gmx-ndi" }));
    l.versions.push(Version {
        version: "1.0.0".into(),
        api,
        platforms: platforms.iter().map(|p| p.to_string()).collect(),
        ..Version::default()
    });
    l
}

#[test]
fn a_listing_runs_here_when_this_platform_and_api_level_are_listed() {
    let level = godwinmix_protocol::API_LEVEL;
    assert!(fit(&versioned(level, &["windows-x86_64"]), "windows-x86_64", false).runs);
    assert!(fit(&versioned(level, &[]), "linux-armv7", false).runs, "no platforms means any");
    let mac_only = fit(&versioned(level, &["macos-aarch64"]), "windows-x86_64", false);
    assert_eq!((mac_only.runs, mac_only.label.as_str()), (false, "Not for Windows"));
    let arm = fit(&versioned(level, &["windows-x86_64"]), "windows-aarch64", false);
    assert_eq!(arm.label, "Not for this computer");
    let newer = fit(&versioned(level + 1, &[]), "windows-x86_64", false);
    assert_eq!((newer.runs, newer.label.as_str()), (false, "Needs a newer GodwinMix"));
}

#[test]
fn a_bundled_listing_runs_when_this_copy_carries_it() {
    let mut camera = versioned(godwinmix_protocol::API_LEVEL, &["macos-aarch64"]);
    camera.display.bundled = true;
    assert!(fit(&camera, "windows-x86_64", true).runs);
    let missing = fit(&camera, "windows-x86_64", false);
    assert_eq!((missing.runs, missing.label.as_str()), (false, "Not in this copy"));
}

#[test]
fn a_listing_for_a_newer_mixer_says_so_and_one_for_this_mixer_runs() {
    let mine = env!("CARGO_PKG_VERSION");
    let later = listing(serde_json::json!({ "name": "x", "source": "a/b", "mixer": "99.0.0" }));
    let fits = fit(&later, "windows-x86_64", false);
    assert!(!fits.runs);
    assert_eq!(fits.label, "Needs a newer GodwinMix");
    assert!(fits.detail.contains("99.0.0"), "{}", fits.detail);
    let now = listing(serde_json::json!({ "name": "x", "source": "a/b", "mixer": mine }));
    assert!(fit(&now, "windows-x86_64", false).runs);
    assert!(newer_than("0.3.10", "0.3.9") && !newer_than("0.3.9", "0.3.10") && !newer_than("", "0.3.9"));
}
