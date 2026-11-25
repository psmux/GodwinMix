//! Layer four: turn the chosen person into the rectangle a camera should
//! show, for a framing style, with headroom above and lead room ahead.

use crate::detect::{Joint, Keypoints};
use crate::geom::Rect;
use crate::track::Track;

/// The shot, as a camera operator would name it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Framing {
    /// Head and shoulders.
    Close,
    /// Head to waist.
    #[default]
    Waist,
    /// The whole body.
    Full,
    /// The person and the space around them.
    Wide,
}

impl Framing {
    /// The slug used on the wire: `close`, `waist`, `full`, `wide`.
    pub fn slug(self) -> &'static str {
        match self {
            Framing::Close => "close",
            Framing::Waist => "waist",
            Framing::Full => "full",
            Framing::Wide => "wide",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        [Framing::Close, Framing::Waist, Framing::Full, Framing::Wide].into_iter().find(|f| f.slug() == s)
    }

    /// Where the shot ends, as a fraction of body height from the top of the
    /// head, when there are no keypoints to say.
    fn cut(self) -> f32 {
        match self {
            Framing::Close => 0.30,
            Framing::Waist => 0.55,
            Framing::Full | Framing::Wide => 1.0,
        }
    }
}

/// How a shot is composed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameStyle {
    pub framing: Framing,
    /// Space above the head, as a fraction of the shot height.
    pub headroom: f32,
    /// How far ahead of a moving person to aim: seconds of their motion,
    /// capped at a quarter of the shot width.
    pub lead_room: f32,
    /// The shot's width over height in pixels. `None` keeps the frame's.
    pub aspect: Option<f32>,
    /// The smallest shot height, normalised: the zoom limit.
    pub min_height: f32,
}

impl Default for FrameStyle {
    fn default() -> Self {
        Self { framing: Framing::Waist, headroom: 0.10, lead_room: 0.5, aspect: None, min_height: 0.15 }
    }
}

/// Where the body ends for this framing, normalised, from keypoints when
/// they are trustworthy.
fn keypoint_cut(k: &Keypoints, framing: Framing) -> Option<f32> {
    let shoulders = k.mid(Joint::LeftShoulder, Joint::RightShoulder, 0.3)?;
    let hips = k.mid(Joint::LeftHip, Joint::RightHip, 0.3);
    let torso = hips.map_or(0.0, |h| h.y - shoulders.y);
    match framing {
        Framing::Close if torso > 0.0 => Some(shoulders.y + 0.35 * torso),
        Framing::Waist => hips.map(|h| h.y + 0.1 * torso),
        Framing::Full | Framing::Wide => k.mid(Joint::LeftAnkle, Joint::RightAnkle, 0.3).map(|a| a.y + 0.03),
        _ => None,
    }
}

/// The shot for `track` in a frame of aspect `frame_aspect` (width over
/// height in pixels). The result may reach outside 0..1: a PTZ camera can
/// pan there; a crop clamps it.
pub fn frame_target(track: &Track, style: &FrameStyle, frame_aspect: f32) -> Rect {
    let b = track.rect;
    // A box much wider than a standing person is somebody cut off by the
    // frame or the furniture: guess their height from their width.
    let px_aspect = b.w * frame_aspect / b.h.max(1e-6);
    let body_h = if px_aspect > 0.6 { b.h.max(b.w * frame_aspect / 0.45) } else { b.h };
    let top = b.y;
    let fallback_cut = top + body_h * style.framing.cut();
    let cut = track.keypoints.and_then(|k| keypoint_cut(&k, style.framing)).unwrap_or(fallback_cut);
    let content = (cut - top).max(1e-3);
    let headroom = style.headroom.clamp(0.0, 0.5);
    let (mut h, mut y) = match style.framing {
        Framing::Wide => {
            let h = content * 2.0;
            (h, top + content / 2.0 - h / 2.0)
        }
        Framing::Full => {
            let h = content / (1.0 - headroom - 0.04);
            (h, top - headroom * h)
        }
        _ => {
            let h = content / (1.0 - headroom);
            (h, top - headroom * h)
        }
    };
    if h < style.min_height {
        y -= (style.min_height - h) / 2.0;
        h = style.min_height;
    }
    let out_aspect = style.aspect.unwrap_or(frame_aspect);
    let w = h * out_aspect / frame_aspect;
    let lead = (track.velocity.x * style.lead_room).clamp(-0.25 * w, 0.25 * w);
    Rect::from_center(b.cx() + lead, y + h / 2.0, w, h)
}
