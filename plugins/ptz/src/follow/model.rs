//! Loading the follow loop's model on a thread of its own: YOLOX nano, and
//! MoveNet Lightning on top for `yolox+pose`. Both are fetched into the
//! models directory on first use and checked by SHA256 (the tracking
//! engine's `ensure_model`).

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use super::detector::{Boxed, Kind};

/// The size YOLOX runs at. The loop's pictures are 320 wide; 320 by 256
/// letterboxes a 16:9 or a 4:3 picture with little waste, and costs about
/// 7 ms on one laptop core (the tracking engine's README has the numbers).
pub const INPUT: (usize, usize) = (320, 256);

/// MoveNet runs on one detection in this many; the tracker carries each
/// person's keypoints in between.
pub const POSE_EVERY: u32 = 3;

/// Start loading `kind` from `dir`. Answers the receiver the model comes
/// back on, and a sentence for `track_state` while it loads.
#[cfg(feature = "yolox")]
pub fn start(kind: Kind, dir: Option<PathBuf>) -> Result<(Receiver<Result<Boxed, String>>, String), String> {
    use godwinmix_track::detect::tract::{models_dir, MOVENET_LIGHTNING, YOLOX_NANO};
    let dir = dir.unwrap_or_else(models_dir);
    let have = dir.join(YOLOX_NANO.file).exists() && (kind != Kind::YoloxPose || dir.join(MOVENET_LIGHTNING.file).exists());
    let why = match have {
        true => format!("motion stands in while {} loads", kind.slug()),
        false => format!("motion stands in while {} downloads into {} (first use only) and loads", kind.slug(), dir.display()),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("ptz-model".into())
        .spawn(move || {
            let _ = tx.send(load(kind, dir));
        })
        .map_err(|e| format!("could not start the model loader: {e}; motion is used"))?;
    Ok((rx, why))
}

#[cfg(not(feature = "yolox"))]
pub fn start(kind: Kind, _dir: Option<PathBuf>) -> Result<(Receiver<Result<Boxed, String>>, String), String> {
    Err(format!("{} was asked for, but this build of gmx-ptz has no model detector (it was built without the `yolox` feature); motion is used", kind.slug()))
}

/// Load `kind` now, blocking: the first time it downloads.
#[cfg(feature = "yolox")]
pub fn load(kind: Kind, dir: PathBuf) -> Result<Boxed, String> {
    use godwinmix_track::detect::tract::{PoseDetector, TractConfig, TractDetector};
    let config = TractConfig { dir: Some(dir.clone()), input: INPUT, ..TractConfig::default() };
    let yolox = TractDetector::new(config).map_err(|e| e.to_string())?;
    if kind != Kind::YoloxPose {
        return Ok(Box::new(yolox));
    }
    let mut pose = PoseDetector::new(yolox, 1, Some(dir)).map_err(|e| e.to_string())?;
    pose.every = POSE_EVERY;
    Ok(Box::new(pose))
}
