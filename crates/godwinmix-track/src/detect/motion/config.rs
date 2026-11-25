//! How the motion detector is tuned.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionConfig {
    /// The width the frame is scaled to before anything else, in pixels.
    pub width: usize,
    /// How far, 0 to 255, a pixel's brightness must move from the
    /// background to count as changed.
    pub threshold: u8,
    /// The background learns 1 / 2^`learn_shift` of each new frame.
    pub learn_shift: u8,
    /// Changed pixels are counted in square blocks of this many pixels.
    pub block: usize,
    /// The fraction of a block that must change for it to count.
    pub block_fill: f32,
    /// Boxes smaller than this, as a fraction of the frame, are dropped.
    pub min_area: f32,
    /// When more than this fraction of the frame changes at once, the light
    /// changed or the camera moved: the background starts again.
    pub max_coverage: f32,
    /// While the camera's picture moves faster than this, in frames a
    /// second (see `EgoMotion::speed`), the detector sees nothing and
    /// leaves the background alone; it learns the new view once the camera
    /// is still again. A slow drift under it is left to the background.
    pub max_camera_speed: f32,
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            width: 160,
            threshold: 24,
            learn_shift: 5,
            block: 4,
            block_fill: 0.25,
            min_area: 0.003,
            max_coverage: 0.5,
            max_camera_speed: 0.02,
        }
    }
}
