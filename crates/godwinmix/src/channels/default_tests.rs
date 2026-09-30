//! The default channel on a real registry over a real channels file: made
//! once, with its one key, not made again after it is removed, and never for
//! a mixer that already has channels or has no file to remember it in.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use godwinmix_core::caps::CanvasCaps;
use godwinmix_core::config::Config;
use godwinmix_core::mixer::Mixer;
use godwinmix_core::plugin::supervisor::Supervisor;
use godwinmix_core::scene::server::SceneServer;
use godwinmix_core::secrets::Secrets;
use godwinmix_protocol::channels::ChannelAddRequest;

use super::super::{Channels, Ports};
use super::ID;

const FILE: &str = "[canvas]\nwidth = 320\nheight = 180\nfps = 30\n\n[multiview]\nenabled = false\n";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-default-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A registry over the runtime store in `dir`, or over none, the way the
/// core opens it. The ingest plugin is not installed in a test, so `open`
/// itself makes nothing; the tests say when it would have been.
fn open(dir: &Path, stored: bool) -> Arc<Channels> {
    let _ = gstreamer::init();
    let path = dir.join("godwinmix.toml");
    std::fs::write(&path, FILE).unwrap();
    let cfg = Config::load(&path).unwrap();
    let (_mix, handle, _cmd, _bus) = Mixer::build(cfg.clone()).unwrap();
    let caps = CanvasCaps::new(&cfg.canvas);
    let secrets: &'static Secrets = Box::leak(Box::new(Secrets::open(&dir.join("secrets")).unwrap()));
    Channels::open(
        stored.then(|| Config::runtime_store_path(&path)),
        Ports::default(),
        Supervisor::new(caps.clone(), Default::default()),
        handle.clone(),
        Arc::new(crate::channels::target::Local { mixer: handle, scenes: SceneServer::in_memory(caps) }),
        secrets,
    )
}

#[tokio::test]
async fn a_fresh_mixer_gets_live_once_and_not_again_after_it_is_removed() {
    let dir = scratch("fresh");
    let channels = open(&dir, true);
    assert!(channels.list().channels.is_empty(), "without the plugin there is no default");
    assert!(channels.make_default(true));
    let list = channels.list();
    assert_eq!(list.channels.len(), 1);
    let live = &list.channels[0];
    assert_eq!((live.id.as_str(), live.name.as_str(), live.app.as_str()), (ID, "Live", "live"));
    assert!(live.enabled && live.auto_source);
    assert_eq!(live.keys.len(), 1);
    assert_eq!(live.keys[0].label, "Default key");
    assert!(!channels.make_default(true), "made once");

    // A restart finds it, and does not make a second.
    let again = open(&dir, true);
    assert!(!again.make_default(true));
    assert_eq!(again.list().channels.len(), 1);

    // Removed, it stays removed across a restart.
    again.remove(ID).unwrap();
    let after = open(&dir, true);
    assert!(!after.make_default(true), "a deleted default is not made again");
    assert!(after.list().channels.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_mixer_with_channels_or_no_file_gets_none() {
    let dir = scratch("had");
    let first = open(&dir, true);
    let req: ChannelAddRequest = serde_json::from_value(serde_json::json!({"name": "Sunday service"})).unwrap();
    first.add(req).unwrap();
    // The file as a core from before the default wrote it: channels, no mark.
    let file = dir.join("godwinmix.runtime.channels.toml");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("default_made"), "no mark yet: {text}");
    let second = open(&dir, true);
    assert!(!second.make_default(true), "a mixer that has channels gets no default");
    second.remove("sunday-service").unwrap();
    let third = open(&dir, true);
    assert!(!third.make_default(true), "nor after its own last channel is removed");

    let bare = scratch("bare");
    assert!(!open(&bare, false).make_default(true), "no file, nowhere to remember a deletion");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&bare).ok();
}

#[tokio::test]
async fn a_second_mixer_on_the_machine_takes_the_live_key_already_sealed() {
    let dir = scratch("shared");
    Secrets::open(&dir.join("secrets")).unwrap().set("channel.live", "default-key", "firstmixerskeywxyz").unwrap();
    let channels = open(&dir, true);
    assert!(channels.make_default(true));
    let key = &channels.list().channels[0].keys[0];
    assert_eq!((key.id.as_str(), key.label.as_str(), key.hint.as_str()), ("default-key", "Default key", "wxyz"));
    std::fs::remove_dir_all(&dir).ok();
}
