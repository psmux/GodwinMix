//! `path.list` and `path.create` against a scratch folder standing in for home.

use super::walk::{self, Refusal};
use std::path::PathBuf;

/// A fresh folder with `shows/sunday`, `shows/.hidden` and a file in it.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-paths-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("shows/sunday")).unwrap();
    std::fs::create_dir_all(dir.join("shows/.hidden")).unwrap();
    std::fs::create_dir_all(dir.join("Archive")).unwrap();
    std::fs::write(dir.join("shows/notes.txt"), "not a folder").unwrap();
    dir
}

fn home(dir: &std::path::Path) -> Vec<(String, PathBuf)> {
    walk::roots(&[("Home", Some(dir.to_path_buf())), ("Missing", Some(dir.join("nope")))])
}

#[test]
fn nothing_asked_lists_home_folders_only_sorted_and_not_hidden() {
    let dir = scratch("home");
    let roots = home(&dir);
    assert_eq!(roots.len(), 1, "a root that does not exist is left out");
    let got = walk::list(&roots, None).unwrap();
    let names: Vec<_> = got.dirs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["Archive", "shows"]);
    assert!(got.writable && got.dirs.iter().all(|d| d.writable));
    assert_eq!(got.parent, None, "home is the top");
    let shows = walk::list(&roots, Some("shows")).unwrap();
    let names: Vec<_> = shows.dirs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["sunday"], "no files, nothing hidden");
    assert_eq!(shows.parent.as_deref(), Some(roots[0].1.display().to_string().as_str()));
}

#[test]
fn it_refuses_to_leave_the_roots() {
    let dir = scratch("out");
    let roots = home(&dir);
    for escape in ["..", "shows/../..", "/"] {
        match walk::list(&roots, Some(escape)) {
            Err(Refusal::Outside(_)) => {}
            other => panic!("{escape} was not refused: {other:?}"),
        }
    }
    let err = super::refusal(walk::list(&roots, Some("/")).unwrap_err());
    assert!(err.message.contains("home folder"), "{}", err.message);
}

#[cfg(unix)]
#[test]
fn a_link_out_of_home_is_neither_offered_nor_followed() {
    let dir = scratch("link");
    std::os::unix::fs::symlink("/", dir.join("escape")).unwrap();
    let roots = home(&dir);
    let got = walk::list(&roots, None).unwrap();
    assert!(got.dirs.iter().all(|d| d.name != "escape"));
    assert!(matches!(walk::list(&roots, Some("escape")), Err(Refusal::Outside(_))));
}

#[test]
fn a_missing_folder_names_the_nearest_one_that_exists() {
    let dir = scratch("missing");
    let roots = home(&dir);
    match walk::list(&roots, Some("shows/easter/2027")) {
        Err(Refusal::Missing { nearest, .. }) => {
            let want = std::fs::canonicalize(dir.join("shows")).unwrap();
            assert_eq!(nearest.as_deref(), Some(want.display().to_string().as_str()));
        }
        other => panic!("{other:?}"),
    }
    let err = super::refusal(walk::list(&roots, Some("shows/easter")).unwrap_err());
    assert_eq!(err.code, -32004);
    assert!(err.data["nearest"].is_string());
}

#[test]
fn create_makes_one_folder_and_lists_it_twice_without_complaint() {
    let dir = scratch("create");
    let roots = home(&dir);
    let parent = walk::list(&roots, Some("shows")).unwrap().path;
    let made = walk::create(&roots, &parent, "Easter").unwrap();
    assert!(made.path.ends_with("Easter") && made.dirs.is_empty());
    assert!(walk::create(&roots, &parent, "Easter").is_ok());
    for bad in ["", "a/b", "..", ".secret", "x:y"] {
        assert!(matches!(walk::create(&roots, &parent, bad), Err(Refusal::BadName(_))), "{bad}");
    }
    assert!(matches!(walk::create(&roots, "/", "Nope"), Err(Refusal::Outside(_))));
}
