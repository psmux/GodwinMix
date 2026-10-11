//! A channel's stream becoming a source when the show answers late: the
//! source the show added after its deadline is the stream's all the same,
//! and goes with it when the encoder stops. Found on a debug build, where a
//! show took more than five seconds over `source.add` and the stream said
//! "Becoming a source" for as long as it was live.

use std::collections::BTreeMap;
use std::sync::Arc;

use godwinmix_protocol::backup::OnLoss;

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
    /// Each source's backup rule, as the show has it.
    rules: Mutex<BTreeMap<String, OnLoss>>,
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
            if let Some(rule) = cfg.on_loss {
                self.rules.lock().insert(cfg.id.clone(), rule);
            }
        }
        Err(self.answer.to_string())
    }
    fn remove_source(&self, id: &str) {
        self.sources.lock().retain(|s| s != id);
        self.rules.lock().remove(id);
    }
    fn rule_of(&self, id: &str) -> Option<OnLoss> {
        self.rules.lock().get(id).cloned()
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
    let show = Arc::new(Show { sources: Mutex::default(), rules: Mutex::default(), adds: true, answer: "show main did not answer source.add within 5 seconds" });
    let channels = open("late", show.clone());
    channels.went_live(&live());
    assert_eq!(stream_source(&channels), (Some("church-main".into()), None));
    channels.went_idle(&json!({"channel": "church", "app": "Church", "stream": "main", "state": "idle"}));
    assert!(show.sources.lock().is_empty(), "the source went with its encoder");
}

/// The operator gave the encoder's source a backup. The encoder stops, the
/// source goes, and when the encoder comes back its source has the backup
/// again; once the backup itself is gone, the rule is not sent.
#[tokio::test]
async fn a_backup_rule_outlives_the_encoder_stopping() {
    let show = Arc::new(Show {
        sources: Mutex::new(vec!["cam-wide".into()]),
        rules: Mutex::default(),
        adds: true,
        answer: "show main did not answer source.add within 5 seconds",
    });
    let channels = open("rule", show.clone());
    let idle = json!({"channel": "church", "app": "Church", "stream": "main", "state": "idle"});
    channels.went_live(&live());
    show.rules.lock().insert("church-main".into(), OnLoss::to("cam-wide"));
    channels.went_idle(&idle);
    assert!(!show.sources.lock().iter().any(|s| s == "church-main"), "the source stayed");
    channels.went_live(&live());
    let back = show.rules.lock().get("church-main").map(|r| r.backup.clone());
    channels.went_idle(&idle);
    show.sources.lock().retain(|s| s != "cam-wide");
    channels.went_live(&live());
    let orphan = show.rules.lock().get("church-main").cloned();
    assert_eq!(back.as_deref(), Some("cam-wide"), "the rule went with the source");
    assert!(orphan.is_none(), "a rule naming a backup that has gone was sent");
}

#[tokio::test]
async fn a_show_that_refused_with_a_reason_is_not_waited_on() {
    let show = Arc::new(Show { sources: Mutex::default(), rules: Mutex::default(), adds: false, answer: "there is no source type ingest/rtmp" });
    let channels = open("refused", show);
    let started = std::time::Instant::now();
    channels.went_live(&live());
    assert!(started.elapsed() < std::time::Duration::from_secs(1), "one look, no waiting");
    let (source, why) = stream_source(&channels);
    assert_eq!(source, None);
    assert_eq!(why.as_deref(), Some("there is no source type ingest/rtmp"));
}
