//! A process the mixer starts while a clip plays must not keep the clip.
//!
//! On Windows a child inherits every inheritable handle, and a clip opened by
//! `filesrc` is one. The camera and screen plugins start with the show, a
//! moment after the sources in the config, so the clips there stayed open in
//! the plugins after `source.remove` and could not be renamed until the show
//! ended. A source added later, with no process starting beside it, was fine.

use super::clip_end::write_clip;
use super::*;

/// A child that lives long enough for the check, started the way a plugin is.
fn long_lived_child() -> std::process::Child {
    let argv: Vec<String> = if cfg!(windows) {
        ["ping", "-n", "60", "127.0.0.1"].map(String::from).to_vec()
    } else {
        ["sleep", "60"].map(String::from).to_vec()
    };
    let spec = crate::input::ExecSpec { argv, env: Default::default(), pipe_stdin: true, cwd: None };
    crate::input::spawn_child(&spec).expect("the child starts")
}

/// Does the child have the file open? Answered where the platform can say:
/// by trying to rename it on Windows, where an open file cannot be, and by
/// reading the child's descriptors on Linux. None where neither works.
fn child_holds(child: &std::process::Child, path: &std::path::Path) -> Option<bool> {
    if cfg!(windows) {
        let moved = path.with_extension("moved");
        return Some(match std::fs::rename(path, &moved) {
            Ok(()) => {
                std::fs::rename(&moved, path).unwrap();
                false
            }
            Err(_) => true,
        });
    }
    let fds = std::fs::read_dir(format!("/proc/{}/fd", child.id())).ok()?;
    let want = std::fs::canonicalize(path).ok()?;
    Some(fds.flatten().any(|f| std::fs::read_link(f.path()).is_ok_and(|t| t == want)))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_process_started_while_a_clip_plays_does_not_keep_it_after_the_source_is_removed() {
    let _ = gst::init();
    let Some(path) = write_clip("child-holds") else {
        println!("skipping: could not write a test clip with jpegenc and avimux");
        return;
    };
    let (mut mix, _handle, _cmds, _bus) = Mixer::build(programme_config(crate::config::Accel::Software)).unwrap();
    mix.start().expect("the programme starts");
    let uri = crate::input::file_uri(&path);
    let src: SourceConfig = toml::from_str(&format!("id = \"clip\"\nuri = \"{uri}\"\n")).unwrap();
    mix.add_source(&src, None).expect("the clip is added");

    let mut child = long_lived_child();
    mix.remove_source(&"clip".into()).expect("the clip is removed");

    // The pipeline stops on a thread of its own, so give it the time it takes.
    let until = Instant::now() + Duration::from_secs(10);
    let mut held = child_holds(&child, &path);
    while held == Some(true) && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(100)).await;
        held = child_holds(&child, &path);
    }
    let _ = child.kill();
    let _ = child.wait();
    mix.shutdown();
    assert_ne!(held, Some(true), "a process started beside the clip still has {} open", path.display());
}
