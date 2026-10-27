//! A headend that followed a Flussonic file under an older version starts
//! under this one on the same folder: the shows and the list of what runs
//! as which show are as the older version left them, whose renditions name
//! no GPU. It keeps following the file (nothing is copied into the
//! station's own channels), every stream keeps its show id and its HLS
//! output, so the Flussonic paths and the segment names in front of it go
//! on as they were, and each rendition is spread over the GPUs from then on.

use super::support::*;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

// Small conversions: on a runner with no GPU each is x264, priced before
// it is made, and the governor turns away what does not fit.
const CONFIG: &str = "stream CH1 {\n  input udp://127.0.0.1:9;\n  transcoder deviceid=0 hw=nvenc vb=200k size=320x180:fit:#000000 ab=64k;\n}\n\
stream CH2 {\n  input udp://127.0.0.1:11;\n  transcoder deviceid=0 hw=nvenc vb=200k size=320x180:fit:#000000 ab=64k;\n}\n";

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["result"]
}

/// Every converted output of every show, by show id: (output id, its gpu).
fn renditions(list: &Value) -> Vec<(String, String, Value)> {
    let mut out = Vec::new();
    for s in list["shows"].as_array().into_iter().flatten() {
        for o in s["outputs"].as_array().into_iter().flatten().filter(|o| o["rendition"].is_object()) {
            out.push((s["id"].as_str().unwrap_or_default().to_string(), o["id"].as_str().unwrap_or_default().to_string(), o["rendition"]["video"]["gpu"].clone()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

async fn until(ws: &mut Ws, mut id: u64, test: impl Fn(&Value) -> bool) -> Value {
    let started = Instant::now();
    loop {
        id += 1;
        let got = ok(&call(ws, id, "show.list", json!({})).await).clone();
        if test(&got) || started.elapsed() > Duration::from_secs(20) {
            return got;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// Take `video.gpu` out of every rendition in a JSON file, as a version
/// from before GPUs could be chosen wrote it.
fn as_before(path: &Path) {
    fn strip(v: &mut Value) {
        match v {
            Value::Object(m) => {
                if let Some(Value::Object(video)) = m.get_mut("video") {
                    video.remove("gpu");
                }
                m.values_mut().for_each(strip);
            }
            Value::Array(a) => a.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    strip(&mut v);
    std::fs::write(path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_old_headend_state_directory_starts_under_the_new_version() {
    let (dir, port) = folder("headend-upgrade");
    // Alone and with no reserve, as headend_import.rs does, so the two
    // conversions fit wherever they can.
    let config = dir.join("godwinmix.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{text}\n[governor]\nreserve_cores = 0\n")).unwrap();
    let conf = dir.join("flussonic.conf");
    std::fs::write(&conf, CONFIG).unwrap();
    let fport = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port().to_string();
    let args = ["headend", "--flussonic", conf.to_str().unwrap(), "--port", &fport];
    let first = {
        let st = start_alone(dir.clone(), port, &args).await;
        let mut ws = rpc(&st, "").await;
        until(&mut ws, 100, |l| renditions(l).len() == 2).await
    };
    let made = renditions(&first);
    if made.is_empty() {
        // What is checked here is the folder, not the governor.
        println!("skipping: this machine had no room for two small conversions, so no show was made to upgrade: {first}");
        return;
    }
    assert_eq!(made.iter().map(|r| (r.0.as_str(), r.1.as_str())).collect::<Vec<_>>(), [("ch1", "flussonic"), ("ch2", "flussonic")], "{first}");

    // The folder as the older version left it.
    for file in ["shows.json", "flussonic-shows.json"] {
        as_before(&dir.join(file));
    }
    assert!(!std::fs::read_to_string(dir.join("flussonic-shows.json")).unwrap().contains("\"gpu\""));

    let st = start_alone(dir.clone(), port, &args).await;
    let mut ws = rpc(&st, "").await;
    let list = until(&mut ws, 200, |l| renditions(l).iter().all(|r| r.2 == "auto") && renditions(l).len() == 2).await;
    let now = renditions(&list);
    assert_eq!(now, vec![("ch1".into(), "flussonic".into(), json!("auto")), ("ch2".into(), "flussonic".into(), json!("auto"))], "same shows, same outputs, spread from now on: {list}");

    let status = ok(&call(&mut ws, 1, "headend.status", json!({})).await).clone();
    assert_eq!(status["following"], true, "{status}");
    let setup = ok(&call(&mut ws, 2, "headend.channel.list", json!({})).await).clone();
    for name in ["CH1", "CH2"] {
        let e = setup["channels"].as_array().unwrap().iter().find(|c| c["channel"]["name"] == name).unwrap();
        assert!(e["follows"].is_string() && e["detached_from"].is_null(), "{name} still follows the file: {setup}");
        assert!(e["channel"].get("gpu").is_none(), "{name} is on auto, so it names no card: {e}");
    }
    let kept = std::fs::read_to_string(dir.join("headend.json")).unwrap_or_default();
    assert!(!kept.contains("CH1"), "nothing is copied into the station's own channels: {kept}");
    let r = reqwest::get(format!("http://127.0.0.1:{fport}/CH1/index.m3u8")).await.unwrap();
    assert_ne!(r.status().as_u16(), 404, "the Flussonic path still answers");
    // A third start on the folder this version wrote changes nothing more.
    drop(ws);
    drop(st);
    let st = start_alone(dir.clone(), port, &args).await;
    let mut ws = rpc(&st, "").await;
    let again = until(&mut ws, 300, |l| renditions(l).len() == 2).await;
    assert_eq!(renditions(&again), now, "{again}");
}
