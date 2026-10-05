//! One history per client: kept apart, and forgotten oldest first.

use super::clients::Histories;
use super::Patch;

#[test]
fn two_clients_have_two_histories() {
    let mut all = Histories::default();
    all.of(Some("default.phone"), 1).push_undo(Patch::default());
    assert_eq!(all.peek(Some("default.phone")).map(|h| h.undo.len()), Some(1));
    assert!(all.peek(Some("default.laptop")).is_none(), "the laptop has done nothing");
}

#[test]
fn the_least_recent_client_is_forgotten_first_and_an_open_transaction_never_is() {
    let mut all = Histories::default();
    all.of(Some("busy"), 0).transaction = Some(Vec::new());
    for n in 1..64u64 {
        all.of(Some(&format!("c{n}")), n);
    }
    all.of(Some("late"), 999);
    assert!(all.peek(Some("busy")).is_some(), "a client mid transaction was forgotten");
    assert!(all.peek(Some("c1")).is_none(), "the oldest idle client should have gone");
    assert!(all.peek(Some("late")).is_some());
}
