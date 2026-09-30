//! When nothing on this machine can make a shape, the nearest one it can:
//! the same codec at a lower frame rate, then smaller, then another codec
//! the container carries at the same size, then smaller.

use godwinmix_protocol::rendition::{Container, EncoderSlot, Fps, VideoShape};

use crate::choose::{choose, Used};
use crate::container::{shape_text, video_codecs, video_name, video_slug};
use crate::error::Suggestion;
use crate::model::CostModel;
use crate::sizing::size;

/// Heights tried on the way down, largest first.
const HEIGHTS: [u32; 7] = [2160, 1440, 1080, 720, 540, 480, 360];

pub fn nearest(
    model: &dyn CostModel,
    encoders: &[EncoderSlot],
    want: &VideoShape,
    container: Container,
    used: &Used,
) -> Option<Suggestion> {
    let mut codecs = vec![want.codec];
    codecs.extend(video_codecs(container).iter().copied().filter(|c| *c != want.codec));
    for codec in codecs {
        let base = VideoShape { codec, ..*want };
        for shape in steps(&base) {
            if let Ok(choice) = choose(model, encoders, &shape, used) {
                return Some(suggest(&shape, &choice.encoder.id));
            }
        }
    }
    None
}

/// The shape asked for, then at half the frame rate when it is over 30, then
/// each smaller standard height at both rates.
fn steps(want: &VideoShape) -> Vec<VideoShape> {
    let mut rates = vec![want.fps];
    if want.fps.as_f64() > 30.5 {
        rates.push(Fps { num: want.fps.num, den: want.fps.den.max(1) * 2 });
    }
    let mut out: Vec<VideoShape> = rates.iter().map(|fps| VideoShape { fps: *fps, ..*want }).collect();
    for h in HEIGHTS.iter().copied().filter(|h| *h < want.height) {
        let (width, height) = size(None, Some(h), want);
        for fps in &rates {
            out.push(VideoShape { width, height, fps: *fps, ..*want });
        }
    }
    out
}

fn suggest(shape: &VideoShape, encoder: &str) -> Suggestion {
    Suggestion {
        codec: video_slug(shape.codec).into(),
        width: shape.width,
        height: shape.height,
        fps: shape.fps,
        encoder: encoder.into(),
        text: format!("{} {} with {encoder}", video_name(shape.codec), shape_text(shape)),
    }
}
