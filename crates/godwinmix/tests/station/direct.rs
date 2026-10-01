//! Shows without compositing (wave 4): kept by the station with no process,
//! their outputs sealed, read back after a restart, and refused where a
//! show process would be needed. No ingest plugin runs here, so no host
//! answers; what the host says is the unit tests' part.

use super::support::*;
use serde_json::{json, Value};

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["result"]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_direct_show_is_kept_with_its_outputs_sealed_and_comes_back_after_a_restart() {
    let (dir, port) = folder("direct-show");
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let added = call(&mut ws, 1, "show.add", json!({
        "name": "BBC One", "compositing": false,
        "input": {"uri": "udp://@239.1.1.1:5000", "program": 101},
        "outputs": [
            {"uri": "srt://10.0.0.9:9000"},
            {"platform": "youtube", "key": "sekret-key-1"},
            {"id": "mc", "uri": "udp://239.2.2.2:5000"}
        ]
    })).await;
    let show = ok(&added);
    assert_eq!(show["id"], "bbc-one");
    assert_eq!(show["compositing"], false);
    assert_eq!(show["state"], "running", "a direct show has no process to start: {show}");
    assert_eq!(show["outputs"].as_array().map(Vec::len), Some(3), "{show}");
    assert_eq!(show["health"]["state"], "alarm", "no host runs here, so its input is missing: {show}");
    assert!(!added.to_string().contains("sekret"), "a key never comes back: {added}");

    let more = call(&mut ws, 2, "show.output.add", json!({"id": "bbc-one", "output": "spare", "uri": "rtmp://h/app/k2"})).await;
    assert_eq!(ok(&more)["outputs"].as_array().map(Vec::len), Some(4));
    let off = call(&mut ws, 3, "show.output.set", json!({"show": "bbc-one", "output": "srt", "enabled": false})).await;
    let srt = ok(&off)["outputs"].as_array().unwrap().iter().find(|o| o["id"] == "srt").cloned().unwrap();
    assert_eq!(srt["enabled"], false, "{srt}");
    let gone = call(&mut ws, 4, "show.output.remove", json!({"id": "bbc-one", "output": "mc"})).await;
    assert_eq!(ok(&gone)["outputs"].as_array().map(Vec::len), Some(3));

    let on_main = call(&mut ws, 5, "show.output.add", json!({"id": "main", "uri": "srt://h:1"})).await;
    assert_eq!(on_main["error"]["data"]["compositing"], true, "{on_main}");
    let mut inside = rpc(&st, "?show=bbc-one").await;
    let asked = call(&mut inside, 6, "scene.list", json!({})).await;
    assert_eq!(asked["error"]["data"]["compositing"], false, "a direct show has no scenes to ask: {asked}");

    let stats = call(&mut ws, 7, "show.stats", json!({"ids": ["bbc-one"]})).await;
    let row = &ok(&stats)["shows"][0];
    assert_eq!(row["id"], "bbc-one");
    assert!(row["outputs"].as_array().unwrap().iter().all(|o| o["rendition_text"] == "copy"), "{row}");
    let narrow = call(&mut ws, 8, "show.stats", json!({"fields": ["health"]})).await;
    assert_eq!(ok(&narrow)["shows"].as_array().map(Vec::len), Some(2), "every show when none is named");
    assert!(ok(&narrow)["shows"][1]["outputs"].as_array().unwrap().is_empty());

    let file = std::fs::read_to_string(dir.join("shows.json")).unwrap();
    assert!(file.contains("\"compositing\": false") && !file.contains("sekret"), "{file}");
    assert!(!dir.join("shows/bbc-one").exists(), "a direct show has no folder");

    drop(ws);
    drop(inside);
    drop(st);
    let st = start(dir, port, &[]).await;
    let list = get(&st, "/api/v1/shows").await;
    let back = list["shows"].as_array().unwrap().iter().find(|s| s["id"] == "bbc-one").cloned().expect("still there");
    assert_eq!(back["compositing"], false);
    assert_eq!(back["input"]["program"], 101);
    let yt = back["outputs"].as_array().unwrap().iter().find(|o| o["id"] == "youtube").cloned().unwrap();
    assert_eq!(yt["has_key"], true, "the key came back from the secret store: {yt}");

    let mut ws = rpc(&st, "").await;
    let removed = call(&mut ws, 9, "show.remove_many", json!({"ids": ["bbc-one", "main", "nope"]})).await;
    let r = ok(&removed);
    assert_eq!(r["removed"], json!(["bbc-one"]));
    assert_eq!(r["refused"].as_array().map(Vec::len), Some(2), "{r}");
    assert_eq!(r["refused"][1]["data"]["kind"], "show", "{r}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_is_checked_whole_priced_on_a_dry_run_and_made_without_half_a_show() {
    let (dir, port) = folder("add-many");
    let st = start(dir, port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let batch = json!([
        {"name": "Feed one", "compositing": false, "input": {"uri": "udp://@239.1.1.1:5000"}, "outputs": [{"uri": "srt://10.0.0.9:9001"}]},
        {"name": "Broken input", "compositing": false, "input": {"uri": "nonsense"}},
        {"name": "Mixed with outputs", "outputs": [{"uri": "srt://10.0.0.9:9002"}]},
        {"name": "Half a show", "compositing": false, "input": {"uri": "udp://@239.1.1.2:5000"},
         "outputs": [{"uri": "srt://10.0.0.9:9003"}, {"platform": "youtube"}]},
        {"name": "Converted", "compositing": false, "input": {"uri": "srt://10.0.0.8:9000"},
         "outputs": [{"uri": "srt://10.0.0.9:9004", "rendition": {"preset": "youtube-720p30"}}]}
    ]);
    let dry = call(&mut ws, 1, "show.add_many", json!({"shows": batch})).await;
    let d = ok(&dry);
    assert_eq!(d["dry_run"], true, "a dry run unless asked otherwise");
    assert_eq!(d["added"][0], "feed-one", "{d}");
    let refused: Vec<u64> = d["refused"].as_array().unwrap().iter().map(|r| r["index"].as_u64().unwrap()).collect();
    assert_eq!(&refused[..3], &[1, 2, 3], "{d}");
    assert_eq!(d["refused"][2]["data"]["field"], "key", "the half show says which output: {d}");
    assert_eq!(d["plan"]["fits"], false);
    // The converting show fits or not by what this machine has free right
    // now, which a busy machine does not; either way it was priced.
    let priced = match d["added"].as_array().unwrap().len() {
        2 => d["plan"]["cost"]["cpu_millicores"].as_u64(),
        _ => {
            assert_eq!(d["refused"][3]["data"]["alarm"], "governor-refused", "{d}");
            d["refused"][3]["data"]["need"]["cpu_millicores"].as_u64()
        }
    };
    assert!(priced.unwrap_or(0) > 0, "the rendition was priced: {d}");
    assert!(d["plan"]["assumed_input"].as_str().unwrap().contains("1920x1080"));
    let list = get(&st, "/api/v1/shows").await;
    assert_eq!(list["shows"].as_array().map(Vec::len), Some(1), "a dry run makes nothing: {list}");

    let many: Vec<Value> = (0..50)
        .map(|i| json!({"name": format!("Channel {i}"), "compositing": false, "input": {"uri": format!("udp://@239.1.2.{i}:5000")},
                        "outputs": [{"uri": format!("srt://10.0.0.9:{}", 10000 + i)}]}))
        .collect();
    let t = std::time::Instant::now();
    let made = call(&mut ws, 2, "show.add_many", json!({"shows": many, "dry_run": false})).await;
    eprintln!("show.add_many of 50 direct shows: {} ms", t.elapsed().as_millis());
    assert_eq!(ok(&made)["added"].as_array().map(Vec::len), Some(50), "{made}");
    let t = std::time::Instant::now();
    for i in 0..20 {
        let s = call(&mut ws, 100 + i, "show.stats", json!({})).await;
        assert_eq!(ok(&s)["shows"].as_array().map(Vec::len), Some(51));
    }
    eprintln!("show.stats of 51 shows: {} us a call", t.elapsed().as_micros() / 20);
    let half = call(&mut ws, 3, "show.add_many", json!({"shows": [batch[3].clone()], "dry_run": false})).await;
    assert!(ok(&half)["added"].as_array().unwrap().is_empty(), "never half a show: {half}");
    let list = get(&st, "/api/v1/shows").await;
    assert!(!list.to_string().contains("half-a-show"), "{list}");
}
