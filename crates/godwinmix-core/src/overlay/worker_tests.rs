//! A render that fails at first is tried again until it draws.

use super::*;
use crate::caps::CanvasCaps;
use crate::config::Canvas;
use std::sync::atomic::{AtomicU32, Ordering};

/// Fails twice, as a first SVG render on a busy machine can, then draws.
fn flaky(tries: &Arc<AtomicU32>, _: Option<(u32, u32)>) -> Result<Rendered> {
    if tries.fetch_add(1, Ordering::SeqCst) < 2 {
        anyhow::bail!("the picture did not decode in ten seconds; is it a picture?");
    }
    let picture = Picture::from_ayuv(vec![255; 4 * 4 * 4], 4, 4, (4, 4));
    Ok(Rendered { picture: Some(picture), motion: Motion::Still, backdrop: None })
}

#[test]
fn a_render_that_failed_is_tried_again_until_it_draws() {
    let _ = gstreamer::init();
    let layer = Layer::new(true);
    let carrier = Arc::new(Carrier::build("worker-test", &CanvasCaps::new(&Canvas::default())).expect("a carrier"));
    let tries = Arc::new(AtomicU32::new(0));
    let tx = spawn("worker-test", tries.clone(), layer.clone(), carrier, flaky);
    // Two failures wait 1 s and then 2 s before the third try draws.
    let until = Instant::now() + Duration::from_secs(8);
    while layer.picture().is_none() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = tx.send(Msg::Stop);
    assert!(layer.picture().is_some(), "still nothing drawn after {} tries", tries.load(Ordering::SeqCst));
    assert_eq!(tries.load(Ordering::SeqCst), 3);
}
