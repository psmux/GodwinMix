use super::entries::{entries, newer, Here};
use godwinmix_host::marketplace::Marketplace;

const API: u32 = godwinmix_protocol::API_LEVEL;

fn market(name: &str, plugins: serde_json::Value) -> Marketplace {
    Marketplace::parse(&serde_json::json!({ "name": name, "owner": "psmux", "plugins": plugins }).to_string())
        .expect("a marketplace")
}

fn community() -> Marketplace {
    market("godwinmix-plugins", serde_json::json!([
        { "name": "camera", "source": "./plugins/camera", "tier": "gold", "title": "Camera",
          "bundled": true, "stars": 8, "kinds": ["source"],
          "versions": [{ "version": "0.1.0", "api": API }] },
        { "name": "clock", "source": "psmux/gmx-clock", "tier": "custom", "stars": 40,
          "thumbnail": "https://x/clock.png",
          "versions": [{ "version": "1.0.0", "api": API, "platforms": ["macos-aarch64"] }] },
        { "name": "ndi", "source": "psmux/gmx-ndi", "tier": "custom",
          "versions": [{ "version": "1.0.0", "api": API }, { "version": "1.2.0", "api": API, "signed": true }] }
    ]))
}

fn here<'a>(installed: &'a [(String, String)], shipped: &'a dyn Fn(&str) -> bool) -> Here<'a> {
    Here { platform: "windows-x86_64", installed, shipped }
}

#[test]
fn a_card_carries_the_listing_and_what_this_machine_makes_of_it() {
    let installed = vec![("ndi".to_string(), "1.0.0".to_string())];
    let shipped = |n: &str| n == "camera";
    let cards = entries(&[community()], &here(&installed, &shipped));
    let names: Vec<&str> = cards.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["camera", "clock", "ndi"], "tier first, then stars");
    let camera = &cards[0];
    assert!(camera.bundled && camera.runs_here && !camera.installed);
    assert_eq!((camera.title.as_str(), camera.install.as_str()), ("Camera", "camera"));
    assert_eq!(camera.author, "psmux", "the marketplace owner when the listing names nobody");
    let clock = &cards[1];
    assert_eq!((clock.runs_here, clock.fit.as_str()), (false, "Not for Windows"));
    assert_eq!(clock.title, "clock", "the name when there is no title");
    let ndi = &cards[2];
    assert!(ndi.installed && ndi.update && ndi.signed);
    assert_eq!((ndi.version.as_str(), ndi.installed_version.as_deref()), ("1.2.0", Some("1.0.0")));
}

#[test]
fn a_plugin_listed_twice_is_shown_once_from_the_higher_tier() {
    let own = market("acme", serde_json::json!([
        { "name": "ndi", "source": "acme/ndi", "tier": "silver" }
    ]));
    let shipped = |_: &str| false;
    let cards = entries(&[community(), own], &here(&[], &shipped));
    let ndi: Vec<_> = cards.iter().filter(|c| c.name == "ndi").collect();
    assert_eq!(ndi.len(), 1);
    assert_eq!((ndi[0].marketplace.as_str(), ndi[0].tier.as_str()), ("acme", "silver"));
}

#[test]
fn a_bundled_plugin_this_copy_lacks_does_not_run_here() {
    let shipped = |_: &str| false;
    let cards = entries(&[community()], &here(&[], &shipped));
    assert_eq!((cards[0].runs_here, cards[0].fit.as_str()), (false, "Not in this copy"));
}

#[test]
fn versions_compare_by_their_numbers() {
    assert!(newer("1.10.0", "1.9.0"));
    assert!(newer("v2.0", "1.99.99"));
    assert!(!newer("1.0.0", "1.0.0"));
    assert!(!newer("1.0.0-beta", "1.0.0"));
    assert!(!newer("", "0.1.0"));
}
