//! What `godwinmix users list` and `users sessions` print.

use godwinmix_protocol::accounts::{SessionInfo, UserInfo};

pub fn users(list: &[UserInfo], json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(list).unwrap_or_default());
        return;
    }
    if list.is_empty() {
        println!("No accounts. The mixer signs people in with its token. Add one with: godwinmix users add <name> --role admin --password-stdin");
        return;
    }
    println!("{:<24} {:<8} {:<9} SESSIONS", "NAME", "ROLE", "STATE");
    for u in list {
        let state = if u.disabled { "disabled" } else { "enabled" };
        println!("{:<24} {:<8} {:<9} {}", u.name, u.role.as_str(), state, u.sessions);
    }
}

pub fn sessions(list: &[SessionInfo], json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(list).unwrap_or_default());
        return;
    }
    if list.is_empty() {
        println!("Nobody is signed in.");
        return;
    }
    println!("{:<14} {:<16} {:<22} {:<16} DEVICE", "ID", "USER", "SIGNED IN", "ADDRESS");
    for s in list {
        let device: String = s.device.chars().take(40).collect();
        println!("{:<14} {:<16} {:<22} {:<16} {}", s.id, s.user, s.created, s.address, device);
    }
}
