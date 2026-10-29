//! One change reaching every screen that asked for it, over real sockets.

use crate::core::Core;
use crate::screen::Screen;
use crate::{state, SCRIPT};
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_change_reaches_every_screen_that_asked_and_no_other_client() {
    let core = Core::start("fanout").await;
    core.ok("prompter.script.save", json!({ "id": "news", "title": "News", "text": SCRIPT })).await;
    let url = core.serve().await;
    let mut glass = Screen::open(&url, &["prompter.*"]).await;
    let mut phone = Screen::open(&url, &["prompter.*"]).await;
    let mut desk = Screen::open(&url, &["source.*", "program.*"]).await;

    desk.call("prompter.load", json!({ "id": "news" })).await.expect("loaded");
    desk.call("prompter.play", json!({ "speed": 1.5 })).await.expect("playing");
    let mut seen = Vec::new();
    for screen in [&mut glass, &mut phone] {
        let loaded = screen.event("prompter.changed", Duration::from_secs(5)).await.expect("the load reached the screen");
        assert_eq!(loaded["script"], "news");
        let played = state(screen.event("prompter.changed", Duration::from_secs(5)).await.expect("the play reached it"));
        assert!(played.playing && played.speed == 1.5);
        seen.push((played.revision, played.at_ms, played.position));
    }
    assert_eq!(seen[0], seen[1], "both screens scroll from the same position, moment and speed");
    assert!(desk.event("prompter.changed", Duration::from_millis(500)).await.is_none(), "a client that did not ask is sent nothing");

    desk.call("prompter.script.save", json!({ "id": "news", "title": "News", "text": "# Opening\nNew words." })).await.expect("saved");
    let told = glass.event("prompter.script.changed", Duration::from_secs(5)).await.expect("the screen is told the words changed");
    assert_eq!(told["id"], "news");
}
