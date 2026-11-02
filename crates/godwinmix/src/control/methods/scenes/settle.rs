//! A still of a scene with filters, taken once the filters have drawn.

use godwinmix_core::multiview::{MultiviewHandle, PreviewSubscription};
use std::sync::Arc;

/// A frame drawn after the preview has every item fed, every filter in
/// place and every keyed one handing the board a picture, or the newest within
/// `SETTLE` of the first frame if that never comes.
///
/// A still at a size nobody was watching builds the preview again at that
/// size, and its first frames go out before the filters are built (off the
/// mixer loop) and before a key or a cutout has handed the preview's board a
/// picture: the still showed the set with the presenter missing. It waited
/// the whole of `SETTLE` every time; now it waits only until the scene is
/// drawn and the frames already in the encoder have gone.
pub(crate) async fn settle(mv: &MultiviewHandle, sub: &mut PreviewSubscription, first: Arc<[u8]>) -> Arc<[u8]> {
    let until = tokio::time::Instant::now() + SETTLE;
    let mut newest = first;
    // Frames composited before the items arrived are still on their way
    // through the encoder when the flags go up, so the still is a few frames
    // after that, not the next one.
    let mut after_drawn = 0;
    loop {
        match tokio::time::timeout_at(until, sub.recv()).await {
            Ok(Ok(frame)) => {
                newest = frame;
                if after_drawn > 0 || mv.preview_filters_drawn() {
                    after_drawn += 1;
                }
                if after_drawn > FRAMES_IN_FLIGHT {
                    return newest;
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            _ => return newest,
        }
    }
}

/// Frames between the compositor and a still: the encoder's queue and the
/// one being encoded, with one to spare.
const FRAMES_IN_FLIGHT: u32 = 3;

/// The longest a still of a scene with filters waits for them.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(1200);
