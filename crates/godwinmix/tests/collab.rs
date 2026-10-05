//! Two people on one token, over two real `/rpc` connections.
//!
//! A phone and a laptop on the same token used to be one client: both were
//! "default", each read the other's edits as its own echo, and Ctrl+Z on one
//! took back what the other had just done. This opens two WebSockets on one
//! core with one token and checks the three things that fixes: each has a
//! client id of its own, each is told when the other comes and goes, and each
//! undoes only its own change.
//!
//! A real server on a real port with a real mixer, as `patches.rs` does,
//! because the ids and the presence stream live in the connection loop.

#[path = "collab/person.rs"]
mod person;
#[path = "collab/server.rs"]
mod server;

use server::serve;
use person::{x_of, Person};
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn two_people_on_one_token_are_two_clients_with_their_own_undo() {
    let url = serve().await;
    let mut phone = Person::open(&format!("{url}?client_id=phone")).await;
    let mut laptop = Person::open(&url).await;

    // Two client ids, one token.
    assert_eq!(phone.client_id, "open.phone", "the name the page chose was not kept");
    assert_ne!(phone.client_id, laptop.client_id, "two connections share a client id");
    let token_of = |id: &str| id.split('.').next().unwrap_or("").to_string();
    assert_eq!(token_of(&phone.client_id), token_of(&laptop.client_id));
    let info = laptop.call("core.info", json!({})).await.expect("core.info");
    assert_eq!(info["client_id"], laptop.client_id.as_str(), "core.info names another client");

    // The phone is told the laptop arrived.
    let laptop_id = laptop.client_id.clone();
    phone.presence_until(|ids| ids.contains(&laptop_id)).await;

    // The laptop says what it is editing, and the phone sees it.
    let listed = laptop.call("presence.set", json!({"scene": "wide"})).await.expect("presence.set");
    let me = listed["clients"].as_array().unwrap().iter().find(|c| c["you"] == true).cloned();
    assert_eq!(me.expect("the caller is marked")["client_id"], laptop.client_id.as_str());

    // Each moves a different box, and each patch says whose it was.
    phone.move_to("left", 100.0).await;
    laptop.move_to("right", 200.0).await;
    let undone = phone.call("scene.undo", json!({})).await.expect("the phone's undo");
    assert_eq!(undone["patch"]["source_client"], phone.client_id.as_str());
    assert_eq!(x_of(&mut phone, "left").await, 0.0, "the phone's move was not undone");
    assert_eq!(x_of(&mut phone, "right").await, 200.0, "the phone undid the laptop's move");

    // Undoing over somebody else's later change is refused, and names them.
    laptop.move_to("left", 300.0).await;
    phone.call("scene.redo", json!({})).await.expect_err("the laptop moved it since");
    phone.move_to("right", 50.0).await;
    laptop.move_to("right", 250.0).await;
    let refused = phone.call("scene.undo", json!({})).await.expect_err("refused");
    assert_eq!(refused["data"]["conflict"], "undo", "{refused}");
    assert_eq!(refused["data"]["conflicts"][0]["changed_by"], laptop.client_id.as_str(), "{refused}");
    phone.call("scene.undo", json!({"force": true})).await.expect("forced");
    assert_eq!(x_of(&mut laptop, "right").await, 0.0);

    // The laptop goes, and the phone is told.
    drop(laptop);
    let phone_id = phone.client_id.clone();
    phone.presence_until(|ids| ids == [phone_id.clone()]).await;
}
