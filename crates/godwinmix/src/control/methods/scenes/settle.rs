//! A still of a scene with filters, taken once the filters have drawn.

/// The newest frame within `SETTLE` of the first.
///
/// A still at a size nobody was watching builds the preview again at that
/// size, and its first frames go out before a keyed item or a cutout has
/// handed the preview's board a picture: the still showed the set with the
/// presenter missing. Frames are kept coming for a moment, which is enough
/// for a key (one frame) and for a cutout sharing a model that is running.
pub(crate) async fn settle(sub: &mut godwinmix_core::multiview::PreviewSubscription, first: std::sync::Arc<[u8]>) -> std::sync::Arc<[u8]> {
    let until = tokio::time::Instant::now() + SETTLE;
    let mut newest = first;
    loop {
        match tokio::time::timeout_at(until, sub.recv()).await {
            Ok(Ok(frame)) => newest = frame,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            _ => return newest,
        }
    }
}

/// How long a still of a scene with filters keeps reading frames.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(1200);
