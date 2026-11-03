//! A template named by its own path, the way a placed set's desk is
//! (`template:C:\Users\...\set-newsroom-desk.svg`), and the hint for a path a
//! shell took the backslashes out of.

use super::pack::{load, shell_hint, PACK};

#[test]
fn a_windows_path_with_its_backslashes_or_forward_slashes_is_read_from_disk() {
    let dir = std::env::temp_dir().join(format!("gmx-pack-path-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("desk.svg");
    let (_, desk) = PACK.iter().find(|(n, _)| *n == "set-newsroom-desk").unwrap();
    std::fs::write(&file, desk).unwrap();
    // On Windows this is C:\Users\...\desk.svg, backslashes and all.
    let native = file.display().to_string();
    let t = load(&native).expect("a template read by its own path");
    assert!(!t.info.fields.is_empty(), "the desk's fields were read from {native}");
    let forward = native.replace('\\', "/");
    load(&forward).expect("the same path with forward slashes");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_path_the_shell_took_the_backslashes_out_of_says_so() {
    assert!(shell_hint("C:Usersgjdesk.svg").contains("single quotes"));
    assert_eq!(shell_hint(r"C:\Users\gj\desk.svg"), "");
    assert_eq!(shell_hint("C:/Users/gj/desk.svg"), "");
    assert_eq!(shell_hint("news-lower-third"), "");
    let e = load("C:Usersgjdesk.svg").unwrap_err().to_string();
    assert!(e.contains("forward slashes"), "{e}");
}
