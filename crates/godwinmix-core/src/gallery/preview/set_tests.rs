//! A studio set's card: its own picture, at once, never its page decoded as
//! a picture, which waited ten seconds and failed.

use super::{preview, Backdrop};
use crate::gallery::store;

#[test]
fn a_shipped_studio_set_shows_its_own_picture_at_once() {
    let _ = gstreamer::init();
    let g = std::env::temp_dir().join(format!("gmx-gallery-sets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&g);
    std::fs::create_dir_all(&g).unwrap();
    let (all, _) = store::list(&g);
    for id in ["studio-newsroom", "studio-ring"] {
        let e = all.iter().find(|e| e.item.id == id).unwrap_or_else(|| panic!("{id} is shipped"));
        let started = std::time::Instant::now();
        let s = preview(&g, e, 240, Backdrop::Checker, &Default::default()).unwrap_or_else(|err| panic!("{id}: {err:#}"));
        assert!(started.elapsed() < std::time::Duration::from_secs(3), "{id} took {:?}", started.elapsed());
        assert!(s.jpeg.starts_with(&[0xff, 0xd8]), "{id} is a JPEG");
    }
    let _ = std::fs::remove_dir_all(&g);
}
