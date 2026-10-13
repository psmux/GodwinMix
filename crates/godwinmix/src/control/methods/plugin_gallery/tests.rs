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

#[test]
fn a_card_carries_its_clips_in_order_and_plays_the_first_that_moves() {
    let docs = [market("m", serde_json::json!([
        { "name": "scripture", "source": "psmux/godwinmix-scripture", "featured": true,
          "screenshots": ["https://x/demo.gif", "https://x/screenshot-1.png"],
          "media": [{ "url": "https://x/demo.gif", "type": "animation" },
                    { "url": "https://x/screenshot-1.png", "type": "image" },
                    { "url": "https://x/tour.mp4", "type": "video" },
                    { "url": "https://x/odd.bin", "type": "hologram" }],
          "versions": [{ "version": "1.0.0", "api": API, "platforms": ["any"] }] },
        { "name": "old", "source": "psmux/old",
          "screenshots": ["https://x/1.png", "https://x/move.GIF?raw=1"] }
    ]))];
    let shipped = |_: &str| false;
    let cards = entries(&docs, &here(&[], &shipped));
    let s = cards.iter().find(|c| c.name == "scripture").unwrap();
    let kinds: Vec<&str> = s.media.iter().map(|m| m.kind.as_str()).collect();
    assert_eq!(kinds, ["animation", "image", "video", "image"], "an unknown type is a picture");
    assert_eq!(s.preview, "https://x/demo.gif");
    assert!(s.featured);
    assert!(s.runs_here, "one asset for any platform runs here: {}", s.fit_detail);
    let old = cards.iter().find(|c| c.name == "old").unwrap();
    let kinds: Vec<&str> = old.media.iter().map(|m| m.kind.as_str()).collect();
    assert_eq!(kinds, ["image", "animation"], "an index with no media is read by extension");
    assert_eq!(old.preview, "https://x/move.GIF?raw=1");
    assert!(!old.featured);
}
