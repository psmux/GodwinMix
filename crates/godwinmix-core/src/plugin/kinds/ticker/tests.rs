use super::*;
use crate::overlay::{Direction, Motion};

fn params(toml_text: &str) -> Params {
    toml::from_str(toml_text).unwrap()
}

#[test]
fn a_ticker_address_is_claimed_and_its_words_are_the_one_item() {
    let kind = |uri: &str| crate::plugin::source::resolve(uri).map(|p| p.manifest.provide_id());
    assert_eq!(kind("ticker:Polls close at ten").as_deref(), Some("ticker/source"));
    let p = validate(&params("uri = \"ticker:Polls close at ten\"")).unwrap();
    assert_eq!(p.words(), vec!["Polls close at ten".to_string()]);
}

#[test]
fn items_join_with_the_separator_and_a_loop_ends_on_one() {
    let p = validate(&params("items = [\"One\", \"Two\"]\nseparator = \" | \"")).unwrap();
    assert_eq!(p.strip(), "One | Two | ", "a loop puts the separator between the end and the start again");
    let once = validate(&params("items = [\"One\", \"Two\"]\nseparator = \" | \"\nloop = false")).unwrap();
    assert_eq!(once.strip(), "One | Two");
    let credits = validate(&params("items = [\"Director\", \"Editor\"]\ndirection = \"up\"")).unwrap();
    assert_eq!(credits.strip(), "Director\nEditor");
    assert!(matches!(credits.motion(), Motion::Crawl { direction: Direction::Up, .. }));
}

#[test]
fn speed_and_direction_change_without_starting_again_and_words_do() {
    let a = validate(&params("text = \"News\"")).unwrap();
    let faster = validate(&params("text = \"News\"\nspeed = 300")).unwrap();
    let other = validate(&params("text = \"Sport\"")).unwrap();
    assert!(!faster.restarts(&a));
    assert!(other.restarts(&a));
    let e = validate(&params("speed = -5")).unwrap_err().to_string();
    assert!(e.contains("params.speed") && e.contains("0 to 5000"), "{e}");
}

#[test]
fn the_strip_is_one_line_at_the_bars_letter_size_and_the_bar_fills_the_box() {
    let _ = gstreamer::init();
    let p = validate(&params("items = [\"Markets up\", \"Rain later\"]\nsize = 30")).unwrap();
    let r = p.render(Some((1280, 60))).unwrap();
    let strip = r.picture.expect("a strip");
    let bar = r.backdrop.expect("a bar");
    assert_eq!((bar.width, bar.height), (1280, 60));
    assert!(strip.height < 60 && strip.width > 200, "{}x{}", strip.width, strip.height);
}
