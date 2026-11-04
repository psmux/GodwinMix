//! The mosaic is built on a worker, not on the loop that serves Take: a take
//! sent right behind the first subscriber's demand answers at once, and
//! sources added and removed while the worker builds come out right.

use super::*;
use crate::multiview::{MultiviewRequest, MultiviewSubscription};

async fn a_frame(sub: &mut MultiviewSubscription) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        match tokio::time::timeout_at(deadline, sub.recv()).await {
            Ok(Ok(_)) => return true,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            _ => return false,
        }
    }
}

fn six_sources() -> crate::config::Config {
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.multiview.enabled = true;
    cfg.multiview.width = 640;
    cfg.multiview.height = 360;
    cfg.multiview.fps = 8;
    cfg.multiview.linger_secs = 0;
    cfg.sources = (1..=6).map(|n| SourceConfig::bare(&format!("cam{n}"), "test://smpte")).collect();
    cfg
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_take_behind_the_first_mosaic_demand_answers_at_once() {
    gst::init().unwrap();
    let (mut mix, handle, commands, _bus) = Mixer::build(six_sources()).unwrap();
    mix.start().unwrap();
    let mv = mix.multiview_handle();
    let thread = spawn(mix, commands, handle.clone());
    handle.request(|ack| Command::Take { source: Some("cam1".into()), at_running_time_ms: None, ack: Some(ack) }).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let asked = Instant::now();
    let mut sub = mv.subscribe(MultiviewRequest::configured());
    // Behind the demand on the queue, and in front of the worker's answer.
    handle.request(|ack| Command::Take { source: Some("cam2".into()), at_running_time_ms: None, ack: Some(ack) }).await.unwrap();
    let take_ms = asked.elapsed().as_millis() as u64;
    // Sources that come and go while the worker builds.
    handle.request(|ack| Command::RemoveSource("cam3".into(), Some(ack))).await.unwrap();
    let late = SourceConfig::bare("cam7", "test://ball");
    handle.request(|ack| Command::AddSourceProbed(Box::new(late), None, Some(ack))).await.unwrap();
    let framed = a_frame(&mut sub).await;
    let built_ms = asked.elapsed().as_millis() as u64;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let status = handle.status().await.unwrap();
    drop(sub);
    handle.send(Command::Shutdown).unwrap();
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();

    println!("take answered in {take_ms} ms, first mosaic frame at {built_ms} ms");
    assert!(framed, "the mosaic produced no frame");
    let slack = crate::plugin::harness::timing_slack();
    assert!((take_ms as f64) < 100.0 * slack, "a take behind the mosaic demand waited {take_ms} ms");
    let mut tiles: Vec<String> = status.multiview.cells.iter().filter_map(|c| c.source.clone()).collect();
    tiles.sort();
    assert_eq!(tiles, ["cam1", "cam2", "cam4", "cam5", "cam6", "cam7"], "a tile for every source here, none for one gone");
}
