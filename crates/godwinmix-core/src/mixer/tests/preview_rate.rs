use super::*;
use crate::multiview::{MultiviewRequest, PreviewRequest};

/// What the preview is built at, once it is built at `fps`, or the last shape seen.
async fn built_at(mv: &crate::multiview::MultiviewHandle, fps: i32) -> Option<i32> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let built = mv.preview_built().map(|shape| shape.fps);
        if built == Some(fps) || tokio::time::Instant::now() > deadline {
            return built;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A page that caps Preview asks for the armed scene at a lower rate, the way
/// `/rpc` does it: the new hold first, then the old one dropped. The mosaic
/// is unchanged, so nothing but the preview's own demand says it should move.
#[tokio::test(flavor = "multi_thread")]
async fn a_preview_asked_at_a_lower_rate_is_rebuilt_at_it() {
    gst::init().unwrap();
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.program.encoder = "on-demand".into();
    cfg.multiview.enabled = true;
    cfg.multiview.width = 320;
    cfg.multiview.height = 180;
    cfg.multiview.fps = 8;
    cfg.multiview.linger_secs = 0;
    cfg.sources = vec![SourceConfig::bare("cam1", "test://smpte")];
    let (mut mix, handle, commands, _bus) = Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let mv = mix.multiview_handle();
    let thread = spawn(mix, commands, handle.clone());

    let mosaic = mv.subscribe(MultiviewRequest { fps: 25, width: 320 });
    let fast = mv.subscribe_preview(PreviewRequest { fps: 25, width: 320, full: false });
    let first = built_at(&mv, 25).await;
    let slow = mv.subscribe_preview(PreviewRequest { fps: 10, width: 320, full: false });
    drop(fast);
    let second = built_at(&mv, 10).await;

    drop(slow);
    drop(mosaic);
    handle.send(Command::Shutdown).unwrap();
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert_eq!(first, Some(25), "the preview is first built at the rate asked");
    assert_eq!(second, Some(10), "and rebuilt at the lower rate once only that is asked");
}
