use super::*;
use crate::multiview::{MultiviewRequest, MultiviewSubscription};

async fn fresh_frame(sub: &mut MultiviewSubscription) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        match tokio::time::timeout_at(deadline, sub.recv()).await {
            Ok(Ok(_)) => return true,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            _ => return false,
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs full canvas sources through repeated live preview attachment"]
async fn live_source_and_preview_churn_keeps_media_flowing() {
    gst::init().unwrap();
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.canvas.width = 1920;
    cfg.canvas.height = 1080;
    cfg.program.encoder = "on-demand".into();
    cfg.multiview.enabled = true;
    cfg.multiview.width = 640;
    cfg.multiview.height = 360;
    cfg.multiview.fps = 8;
    cfg.multiview.linger_secs = 0;
    cfg.sources = vec![SourceConfig::bare("permanent", "test://smpte")];
    let (mut mix, handle, commands, _bus) = Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let mv = mix.multiview_handle();
    let thread = spawn(mix, commands, handle.clone());
    let mut sub = mv.subscribe(MultiviewRequest::configured());
    let mut failure = None;
    if !fresh_frame(&mut sub).await {
        failure = Some("initial preview did not produce a frame".to_string());
    }
    for n in 0..12 {
        if failure.is_some() {
            break;
        }
        let source = SourceConfig::bare(&format!("churn-{n}"), "test://ball");
        handle.request(|ack| Command::AddSourceProbed(Box::new(source.clone()), None, Some(ack))).await.unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
        let status = handle.status().await.unwrap();
        let source_status = status.sources.iter().find(|s| s.id == source.id).unwrap();
        if source_status.video_idle_ms.is_none_or(|idle| idle > 1_000) {
            failure = Some(format!("round {n}: source stopped delivering: {source_status:?}"));
        }
        if !fresh_frame(&mut sub).await {
            failure = Some(format!("round {n}: preview stopped delivering"));
        }
        handle.request(|ack| Command::RemoveSource(source.id, Some(ack))).await.unwrap();
        drop(sub);
        tokio::time::sleep(Duration::from_millis(100)).await;
        sub = mv.subscribe(MultiviewRequest::configured());
        if !fresh_frame(&mut sub).await {
            failure = Some(format!("round {n}: reopened preview missed its deadline"));
        }
    }
    drop(sub);
    handle.send(Command::Shutdown).unwrap();
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert!(failure.is_none(), "{}", failure.unwrap());
}
