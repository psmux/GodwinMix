//! A channel's stream becoming a source when the show answers late: the
//! source the show added after its deadline is the stream's all the same,
//! and goes with it when the encoder stops. Found on a debug build, where a
//! show took more than five seconds over `source.add` and the stream said
//! "Becoming a source" for as long as it was live.

use std::sync::Arc;

use godwinmix_core::caps::CanvasCaps;
use godwinmix_core::config::{Config, SourceConfig};
use godwinmix_core::mixer::Mixer;
use godwinmix_core::plugin::supervisor::Supervisor;
use godwinmix_core::secrets::Secrets;
use parking_lot::Mutex;
use serde_json::json;

use super::super::target::Programme;
use super::super::{Channels, Ports};

const FILE: &str = "[canvas]\nwidth = 320\nheight = 180\nfps = 30\n\n[multiview]\nenabled = false\n";

/// A show that does what it is asked but answers with `answer`, or adds
/// nothing at all when `adds` is false.
struct Show {
    sources: Mutex<Vec<String>>,
    adds: bool,
    answer: &'static str,
}

impl Programme for Show {
    fn has_source(&self, id: &str) -> Option<bool> {
        Some(self.sources.lock().iter().any(|s| s == id))
    }
    fn holds(&self, _id: &str) -> bool {
        false
    }
    fn add_source(&self, cfg: SourceConfig) -> Result<(), String> {
        if self.adds {
            self.sources.lock().push(cfg.id.clone());
        }
        Err(self.answer.to_string())
    }
    fn remove_source(&self, id: &str) {
        self.sources.lock().retain(|s| s != id);
    }
}

fn open(name: &str, show: Arc<Show>) -> Arc<Channels> {
    let _ = gstreamer::init();
    let dir = std::env::temp_dir().join(format!("gmx-auto-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("godwinmix.toml");
    std::fs::write(&path, FILE).unwrap();
    let cfg = Config::load(&path).unwrap();
    let (_mix, handle, _cmd, _bus) = Mixer::build(cfg.clone()).unwrap();
    let caps = CanvasCaps::new(&cfg.canvas);
    let secrets: &'static Secrets = Box::leak(Box::new(Secrets::open(&dir.join("secrets")).unwrap()));
    let channels = Channels::open(None, Ports::default(), Supervisor::new(caps, Default::default()), handle, show, secrets);
    channels.add(serde_json::from_value(json!({"name": "Church", "app": "Church"})).unwrap()).unwrap();
    channels
}

fn stream_source(channels: &Channels) -> (Option<String>, Option<String>) {
    let list = channels.list();
    let s = &list.channels[0].streams[0];
    (s.source.clone(), s.source_error.clone())
}

fn live() -> serde_json::Value {
    json!({"channel": "church", "app": "Church", "stream": "main", "state": "live", "from": "127.0.0.1:5000", "protocol": "rtmp"})
}

#[tokio::test]
async fn a_source_the_show_added_after_its_deadline_is_the_streams_and_goes_with_it() {
    let show = Arc::new(Show { sources: Mutex::default(), adds: true, answer: "show main did not answer source.add within 5 seconds" });
    let channels = open("late", show.clone());
    channels.went_live(&live());
    assert_eq!(stream_source(&channels), (Some("church-main".into()), None));
    channels.went_idle(&json!({"channel": "church", "app": "Church", "stream": "main", "state": "idle"}));
    assert!(show.sources.lock().is_empty(), "the source went with its encoder");
}

#[tokio::test]
async fn a_show_that_refused_with_a_reason_is_not_waited_on() {
    let show = Arc::new(Show { sources: Mutex::default(), adds: false, answer: "there is no source type ingest/rtmp" });
    let channels = open("refused", show);
    let started = std::time::Instant::now();
    channels.went_live(&live());
    assert!(started.elapsed() < std::time::Duration::from_secs(1), "one look, no waiting");
    let (source, why) = stream_source(&channels);
    assert_eq!(source, None);
    assert_eq!(why.as_deref(), Some("there is no source type ingest/rtmp"));
}
