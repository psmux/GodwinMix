//! The invites file: what is kept, what is dropped, and who may read it.

use super::store::{self, Record};

fn record(id: &str, expires_ms: u64) -> Record {
    Record { id: id.into(), name: id.into(), created: "2026-10-29T10:00:00.000Z".into(), expires_ms, key: format!("{id}-key") }
}

#[test]
fn invites_come_back_less_the_ones_that_ran_out() {
    let dir = std::env::temp_dir().join(format!("gmx-guest-store-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = store::path_beside(&dir.join("godwinmix.toml"));
    assert!(path.ends_with("godwinmix.guests.json"), "{}", path.display());
    store::write(&path, &[record("ana", 2_000), record("ben", 500)]).expect("written");
    let back = store::load(&path, 1_000);
    assert_eq!(back, vec![record("ana", 2_000)], "ben ran out at 500");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the file holds keys, so only its owner reads it");
    }
    std::fs::write(&path, "{not json").unwrap();
    assert!(store::load(&path, 0).is_empty());
    assert!(path.with_extension("json.unreadable").exists(), "a broken file is moved aside, not written over");
    std::fs::remove_dir_all(&dir).ok();
}
