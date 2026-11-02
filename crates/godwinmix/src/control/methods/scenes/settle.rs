//! A still of a scene with filters, taken once the filters have drawn.

use godwinmix_core::multiview::{MultiviewHandle, PreviewSubscription};
use std::sync::Arc;

/// The first frame drawn after the preview has every filter in place and
/// every keyed one has handed the board a picture, or the newest within
/// `SETTLE` of the first frame if that never comes.
///
/// A still at a size nobody was watching builds the preview again at that
/// size, and its first frames go out before the filters are built (off the
/// mixer loop) and before a key or a cutout has handed the preview's board a
/// picture: the still showed the set with the presenter missing. It waited
/// the whole of `SETTLE` every time; now it waits only until the filters
/// have drawn, which for a key on a running preview is a frame or two.
pub(crate) async fn settle(mv: &MultiviewHandle, sub: &mut PreviewSubscription, first: Arc<[u8]>) -> Arc<[u8]> {
    let until = tokio::time::Instant::now() + SETTLE;
    let mut newest = first;
    // The frame after the one during which the filters were seen drawn, so
    // the still is one that was composited with them.
    let mut drawn = false;
    loop {
        match tokio::time::timeout_at(until, sub.recv()).await {
            Ok(Ok(frame)) => {
                newest = frame;
                if drawn {
                    return newest;
                }
                drawn = mv.preview_filters_drawn();
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            _ => return newest,
        }
    }
}

/// The longest a still of a scene with filters waits for them.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(1200);
