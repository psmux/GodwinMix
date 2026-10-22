//! Every group this process has open, so a second show on a group shares
//! the first one's socket instead of joining again.
//!
//! The map's lock is held to find or add a group and nothing else: the
//! socket is opened on the group's own thread.

use std::sync::{Arc, Mutex, OnceLock};

use super::group::{Group, Key, Subscription};
use super::pump;

fn groups() -> &'static Mutex<Vec<Arc<Group>>> {
    static GROUPS: OnceLock<Mutex<Vec<Arc<Group>>>> = OnceLock::new();
    GROUPS.get_or_init(Mutex::default)
}

/// Take `program` (or the first) from the group at `key`, opening it if no
/// show has it open. Never waits on the network.
pub fn subscribe(key: &Key, address: &str, buffer: usize, program: Option<u16>) -> Subscription {
    let mut all = groups().lock().unwrap_or_else(|e| e.into_inner());
    all.retain(|g| !g.subs().closed);
    for g in all.iter().filter(|g| &g.key == key) {
        if let Some((id, rx, dropped)) = g.add(program) {
            return Subscription { group: g.clone(), id, rx, dropped };
        }
    }
    let group = Arc::new(Group::new(key.clone(), address.to_string()));
    let (id, rx, dropped) = group.add(program).expect("a new group is open");
    let (g, name) = (group.clone(), format!("gmx-ts-{}", key.port));
    let started = std::thread::Builder::new().name(name).spawn(move || pump::run(g, buffer));
    if let Err(e) = started {
        group.set_error(Some(format!("could not start the receive thread for {address}: {e}")));
    }
    all.push(group.clone());
    Subscription { group, id, rx, dropped }
}

/// What every open group is doing, for the wall: one row per socket.
pub fn snapshot() -> Vec<serde_json::Value> {
    let all = groups().lock().unwrap_or_else(|e| e.into_inner()).clone();
    all.iter().map(|g| super::stats::group_json(g)).collect()
}

/// How many groups are open, for the tests.
pub fn open_count() -> usize {
    groups().lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|g| !g.subs().closed).count()
}
