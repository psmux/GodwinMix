//! `path.list` and `path.create` against a scratch folder standing in for home.

use super::places;
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
    walk::roots(&[("Home", Some(dir.to_path_buf())), ("Missing", Some(dir.join("nope")))], vec![])
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
    let shows = walk::list(&roots, Some("shows")).unwrap();
    let names: Vec<_> = shows.dirs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["sunday"], "no files, nothing hidden");
    assert_eq!(shows.parent.as_deref(), Some(places::shown(&roots[0].1).as_str()));
}

#[test]
fn up_one_level_goes_past_home_to_the_top_of_the_drive() {
    let dir = scratch("up");
    let roots = home(&dir);
    let mut at = walk::list(&roots, None).unwrap();
    assert!(at.parent.is_some(), "home has a folder above it");
    let mut steps = 0;
    while let Some(parent) = at.parent.clone() {
        at = walk::list(&roots, Some(&parent)).unwrap();
        steps += 1;
        assert!(steps < 64, "never reached the top");
    }
    assert!(!at.path.starts_with(r"\\?\"), "{}", at.path);
    let drive = at.path.len() == 3 && places::drive_top(&at.path[..2]).as_deref() == Some(at.path.as_str());
    assert!(at.path == "/" || drive, "{}", at.path);
    assert!(at.dirs.iter().all(|d| !d.path.starts_with(r"\\?\") && !d.name.starts_with('.')));
}

#[cfg(windows)]
#[test]
fn paths_come_back_without_the_verbatim_prefix_and_without_hidden_system_folders() {
    let dir = scratch("verbatim");
    let roots = home(&dir);
    let got = walk::list(&roots, Some("shows")).unwrap();
    let want = format!("{}\\shows", places::simplify(&std::fs::canonicalize(&dir).unwrap().display().to_string()));
    assert_eq!(got.path, want);
    assert!(got.roots.iter().all(|r| !r.path.starts_with(r"\\?\")));
    let top = walk::list(&roots, Some("c:")).unwrap();
    assert_eq!(top.path, r"C:\");
    assert_eq!(top.parent, None);
    assert!(top.dirs.iter().all(|d| d.name != "$Recycle.Bin" && d.name != "System Volume Information"));
}

#[test]
fn simplify_takes_the_prefix_off_only_when_the_path_means_the_same_without_it() {
    let cases = [
        (r"\\?\C:\Users\you\Videos\GodwinMix", r"C:\Users\you\Videos\GodwinMix"),
        (r"\\?\D:\", r"D:\"),
        (r"\\?\UNC\nas\shows\Easter", r"\\nas\shows\Easter"),
        (r"C:\Already\Plain", r"C:\Already\Plain"),
        ("/home/show/Videos", "/home/show/Videos"),
        (r"\\?\C:\Shows\CON", r"\\?\C:\Shows\CON"),
        (r"\\?\C:\Shows\lpt1.txt", r"\\?\C:\Shows\lpt1.txt"),
        (r"\\?\C:\Shows\trailing.", r"\\?\C:\Shows\trailing."),
        (r"\\?\C:\Shows\trailing ", r"\\?\C:\Shows\trailing "),
        (r"\\?\Volume{1234}\Shows", r"\\?\Volume{1234}\Shows"),
        (r"\\?\GLOBALROOT\Device", r"\\?\GLOBALROOT\Device"),
    ];
    for (raw, want) in cases {
        assert_eq!(places::simplify(raw), want, "{raw}");
    }
    let long = format!(r"\\?\C:\{}", "a".repeat(300));
    assert_eq!(places::simplify(&long), long, "too long to work without the prefix");
    assert_eq!(places::simplify(r"\\?\C:\Shows\Console"), r"C:\Shows\Console", "only the device names themselves");
}

#[test]
fn a_bare_drive_letter_is_the_top_of_the_drive() {
    assert_eq!(places::drive_top("d:").as_deref(), Some(r"D:\"));
    assert_eq!(places::drive_top("D:").as_deref(), Some(r"D:\"));
    for not in ["D", r"D:\", "DD:", "1:", "/d"] {
        assert_eq!(places::drive_top(not), None, "{not}");
    }
}

#[test]
fn drive_letters_come_from_the_mask_in_order() {
    assert!(places::letters(0).is_empty());
    assert_eq!(places::letters(0b1100), [r"C:\", r"D:\"]);
    assert_eq!(places::letters(1 | 1 << 25 | 1 << 31), [r"A:\", r"Z:\"], "bits past Z mean nothing");
}

#[test]
fn the_system_places_are_drives_on_windows_and_the_root_elsewhere() {
    let got = places::system();
    if cfg!(windows) {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        assert!(got.iter().any(|(label, _)| label.eq_ignore_ascii_case(&system)), "{got:?}");
        assert!(got.iter().all(|(label, p)| p.display().to_string() == format!("{label}\\")));
    } else {
        assert_eq!(got[0], ("Computer".to_string(), PathBuf::from("/")));
    }
}

#[test]
fn mounted_volumes_are_the_folders_inside_the_bases_and_not_the_bases() {
    let dir = scratch("volumes");
    let media = dir.join("media");
    let mine = media.join("show");
    std::fs::create_dir_all(mine.join("USB STICK")).unwrap();
    std::fs::create_dir_all(media.join("Backup")).unwrap();
    std::fs::create_dir_all(media.join(".hidden")).unwrap();
    std::fs::write(media.join("file.txt"), "no").unwrap();
    let got = places::mounted(&[mine.clone(), media.clone(), dir.join("absent")]);
    let names: Vec<_> = got.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Backup", "USB STICK"]);
}

#[test]
fn a_missing_folder_names_the_nearest_one_that_exists() {
    let dir = scratch("missing");
    let roots = home(&dir);
    match walk::list(&roots, Some("shows/easter/2027")) {
        Err(Refusal::Missing { nearest, .. }) => {
            let want = places::shown(&std::fs::canonicalize(dir.join("shows")).unwrap());
            assert_eq!(nearest.as_deref(), Some(want.as_str()));
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
}
