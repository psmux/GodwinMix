//! What can be tested without a card: the manifest, the pipeline a card
//! would get (parsed by GStreamer, so every element and property name is
//! checked), and that a machine with no card lists nothing and says why
//! rather than failing.

use crate::card;
use crate::settings::Settings;
use godwinmix_sdk::prelude::*;
use godwinmix_sdk::wire::Transport;
use gstreamer as gst;

#[test]
fn the_shipped_manifest_passes_the_validator_the_harness_runs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let m = Manifest::load(root.join("gmx-plugin.toml")).expect("the manifest must validate");
    assert_eq!(m.provides.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["source", "devices"]);
}

#[test]
fn the_pipeline_a_card_would_get_parses_with_its_sound_or_without() {
    godwinmix_capture_common::init().unwrap();
    if card::missing_element().is_some() {
        eprintln!("skipping: this GStreamer has no decklink elements");
        return;
    }
    let canvas = Canvas { width: 1920, height: 1080, fps: 30 };
    let s = Settings::from_params(&serde_json::json!({"device_number": 1, "connection": "sdi", "mode": "1080i50"})).unwrap();
    let description = card::wiring(&s, canvas).description(Transport::Container).unwrap();
    assert!(description.contains("decklinkaudiosrc device-number=1"), "{description}");
    let pipeline = gst::parse::launch(&description).expect("GStreamer parses what a card would get");
    drop(pipeline);
    let quiet = Settings::from_params(&serde_json::json!({"audio": false})).unwrap();
    assert!(!card::wiring(&quiet, canvas).description(Transport::Container).unwrap().contains("decklinkaudiosrc"));
}

/// This machine has the elements and no card. Discovery is empty, not an
/// error, and opening input 0 says what to check.
#[test]
fn with_no_card_nothing_is_listed_and_opening_one_says_what_to_check() {
    godwinmix_capture_common::init().unwrap();
    if card::missing_element().is_some() {
        eprintln!("skipping: this GStreamer has no decklink elements");
        return;
    }
    if !card::inputs().is_empty() {
        eprintln!("skipping: this machine has a DeckLink card, which this test is about the absence of");
        return;
    }
    let s = Settings::default();
    let canvas = Canvas { width: 1280, height: 720, fps: 30 };
    let pipeline = card::build(&s, canvas, Transport::Container, "").expect("the pipeline builds without a card");
    let err = gstreamer::prelude::ElementExt::set_state(&pipeline, gst::State::Playing).map(|_| ()).map_err(|e| e.to_string());
    let _ = gstreamer::prelude::ElementExt::set_state(&pipeline, gst::State::Null);
    let said = card::explain(&err.err().unwrap_or_else(|| "failed to acquire input".into()), &s);
    assert!(said.contains("Desktop Video"), "{said}");
}
