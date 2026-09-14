use super::*;

/// `GODWINMIX_HOME` is process wide, so the tests that write a store take this
/// in turn. Everything else here is pure and runs in parallel.
static HOME: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Home {
    dir: PathBuf,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Home {
    fn new(name: &str) -> Self {
        let guard = HOME.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("gmx-market-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a home");
        std::env::set_var("GODWINMIX_HOME", &dir);
        Self { dir, _guard: guard }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        std::env::remove_var("GODWINMIX_HOME");
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn document(name: &str, plugins: serde_json::Value) -> String {
    serde_json::json!({
        "name": name,
        "title": "A marketplace",
        "owner": "psmux",
        "version": 1,
        "plugins": plugins
    })
    .to_string()
}

fn write_market(at: &Path, name: &str, plugins: serde_json::Value) -> PathBuf {
    std::fs::create_dir_all(at).expect("a directory");
    let file = at.join("godwinmix-marketplace.json");
    std::fs::write(&file, document(name, plugins)).expect("a document");
    file
}

#[test]
fn a_document_with_every_field_parses_and_keeps_what_this_core_does_not_read() {
    let text = serde_json::json!({
        "name": "official",
        "plugins": [{
            "name": "ndi",
            "source": "psmux/gmx-ndi",
            "description": "NDI in and out",
            "tier": "gold",
            "kinds": ["source", "output"],
            "versions": [
                { "version": "1.0.0", "api": 1, "platforms": ["linux-x86_64"], "signed": true },
                { "version": "1.2.0", "api": 1, "platforms": ["linux-x86_64"], "signed": true,
                  "harness": ["linux-x86_64: 8/8"] }
            ],
            "badge": "https://example.invalid/badge.json"
        }]
    })
    .to_string();
    let market = Marketplace::parse(&text).expect("it parses");
    assert_eq!(market.plugins.len(), 1);
    let ndi = market.find("ndi").expect("ndi is listed");
    assert_eq!(ndi.tier, Tier::Gold);
    assert_eq!(ndi.usable_version().map(|v| v.version.as_str()), Some("1.2.0"));
    assert!(ndi.extra.contains_key("badge"), "a field this core ignores survives");
    assert!(matches!(ndi.source().expect("a source"), Source::GitHub { .. }));
}

#[test]
fn a_listing_whose_source_is_not_a_source_is_refused_with_the_plugin_named() {
    let text = serde_json::json!({
        "name": "broken",
        "plugins": [{ "name": "ndi", "source": "just-a-word" }]
    })
    .to_string();
    let err = Marketplace::parse(&text).expect_err("the source does not parse");
    let message = format!("{err:#}");
    assert!(message.contains("ndi"), "{message}");
    assert!(message.contains("broken"), "{message}");
}

#[test]
fn a_schema_version_from_the_future_says_to_upgrade() {
    let text = serde_json::json!({ "name": "x", "version": 9, "plugins": [] }).to_string();
    let err = Marketplace::parse(&text).expect_err("version 9 is not readable");
    assert!(format!("{err}").contains("Upgrade GodwinMix"), "{err}");
}

#[test]
fn a_version_whose_api_this_core_cannot_run_is_not_chosen() {
    let text = serde_json::json!({
        "name": "x",
        "plugins": [{
            "name": "ndi",
            "source": "psmux/gmx-ndi",
            "versions": [
                { "version": "1.0.0", "api": 1 },
                { "version": "2.0.0", "api": 99 }
            ]
        }]
    })
    .to_string();
    let market = Marketplace::parse(&text).expect("it parses");
    let ndi = market.find("ndi").expect("listed");
    assert_eq!(
        ndi.usable_version().map(|v| v.version.as_str()),
        Some("1.0.0"),
        "the newest version this core can run wins, not simply the newest"
    );
}

#[test]
fn add_list_and_remove_go_round_the_loop() {
    let home = Home::new("loop");
    let repo = home.dir.join("repo");
    write_market(
        &repo,
        "local",
        serde_json::json!([{ "name": "clock", "source": "psmux/gmx-clock", "tier": "bronze" }]),
    );

    let added = add(&repo.display().to_string()).expect("it adds");
    assert_eq!(added.name, "local");
    assert_eq!(added.plugins, 1);
    assert!(cache_path("local").is_file(), "the document is cached");

    let store = load_store();
    assert_eq!(store.marketplaces.len(), 1);

    let hits = search("clock", &[]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, "local");

    let gone = remove("local").expect("it removes");
    assert_eq!(gone.name, "local");
    assert!(load_store().marketplaces.is_empty());
    assert!(!cache_path("local").is_file(), "the cache goes with it");
}

#[test]
fn removing_one_that_was_never_added_lists_what_was() {
    let _home = Home::new("missing");
    let err = remove("nope").expect_err("nothing was added");
    assert!(format!("{err}").contains("Added: none"), "{err}");
}

#[test]
fn a_directory_with_no_document_says_what_a_marketplace_is() {
    let home = Home::new("nodoc");
    let empty = home.dir.join("empty");
    std::fs::create_dir_all(&empty).expect("a directory");
    let err = add(&empty.display().to_string()).expect_err("there is no document");
    assert!(format!("{err:#}").contains("godwinmix-marketplace.json"), "{err:#}");
}

#[test]
fn a_higher_tier_wins_when_two_marketplaces_list_the_same_plugin() {
    let home = Home::new("tiers");
    let community = home.dir.join("community");
    std::fs::write(
        {
            std::fs::create_dir_all(&community).expect("a directory");
            community.join("godwinmix-marketplace.json")
        },
        document(
            "community",
            serde_json::json!([{ "name": "ndi", "source": "someone/gmx-ndi", "tier": "bronze" }]),
        ),
    )
    .expect("a document");
    let official = home.dir.join("official");
    write_market(
        &official,
        "official",
        serde_json::json!([{ "name": "ndi", "source": "psmux/gmx-ndi", "tier": "gold" }]),
    );

    add(&community.display().to_string()).expect("it adds");
    add(&official.display().to_string()).expect("it adds");

    let (market, listing) = resolve("ndi", &[]).expect("ndi resolves");
    assert_eq!(market.name, "official");
    assert_eq!(listing.source, "psmux/gmx-ndi");
}

#[test]
fn pinning_with_only_hides_every_other_marketplace() {
    let home = Home::new("pinned");
    let one = home.dir.join("one");
    write_market(
        &one,
        "one",
        serde_json::json!([{ "name": "a", "source": "x/gmx-a" }]),
    );
    let two = home.dir.join("two");
    write_market(
        &two,
        "two",
        serde_json::json!([{ "name": "b", "source": "x/gmx-b" }]),
    );
    add(&one.display().to_string()).expect("it adds");
    add(&two.display().to_string()).expect("it adds");

    let pinned = vec!["two".to_string()];
    assert_eq!(documents(&pinned).len(), 1);
    assert!(resolve("a", &pinned).is_none(), "a is not in the pinned marketplace");
    assert!(resolve("b", &pinned).is_some());
    assert_eq!(search("", &pinned).len(), 1);
}

#[test]
fn owner_slash_repo_becomes_a_raw_github_url_for_both_file_names() {
    let urls = candidate_urls("psmux/godwinmix-plugins").expect("it makes URLs");
    assert_eq!(urls.len(), 2);
    assert!(urls[0].ends_with("/psmux/godwinmix-plugins/HEAD/godwinmix-marketplace.json"), "{urls:?}");
    assert!(urls[1].ends_with("/psmux/godwinmix-plugins/HEAD/index.json"), "{urls:?}");
}

#[test]
fn a_url_that_already_names_a_json_file_is_used_as_it_is() {
    let urls = candidate_urls("https://example.invalid/mine.json").expect("it makes URLs");
    assert_eq!(urls, vec!["https://example.invalid/mine.json".to_string()]);
}

#[test]
fn a_bare_word_is_not_a_marketplace() {
    let err = candidate_urls("plugins").expect_err("one segment is not owner/repo");
    assert!(format!("{err}").contains("owner/repo"), "{err}");
}

#[test]
fn the_tiers_sort_the_way_06_section_4_orders_them() {
    let mut tiers = vec![Tier::Bronze, Tier::Gold, Tier::Custom, Tier::Silver];
    tiers.sort();
    assert_eq!(tiers, vec![Tier::Custom, Tier::Bronze, Tier::Silver, Tier::Gold]);
    assert_eq!(Tier::Custom.label(), "custom, unreviewed");
}
