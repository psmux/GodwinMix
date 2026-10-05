//! The two scenes into one stacked frame for the GPU, and the answer back
//! onto the programme frame.

use super::super::super::frame::Pic;
use crate::overlay::blend::Planes;
use gstreamer as gst;
use gstreamer_video as gst_video;

/// One buffer twice the canvas height, old picture above new.
pub fn stack(stacked: &gst_video::VideoInfo, old: &Pic<'_>, f: &Planes<'_>) -> Option<gst::Buffer> {
    let mut buffer = gst::Buffer::with_size(stacked.size()).ok()?;
    {
        let b = buffer.get_mut()?;
        let mut frame = gst_video::VideoFrameRef::from_buffer_ref_writable(b, stacked).ok()?;
        let (w, h) = (f.width as usize, f.height as usize);
        let s = stacked.stride();
        let planes = frame.planes_data_mut();
        let [y, u, v, _] = planes;
        let new = [&*f.y, &*f.u, &*f.v];
        for (i, (dst, rows, cols)) in [(y, h, w), (u, h / 2, w / 2), (v, h / 2, w / 2)].into_iter().enumerate() {
            let ds = s[i] as usize;
            let olds = [old.y, old.u, old.v][i];
            for r in 0..rows {
                dst[r * ds..r * ds + cols].copy_from_slice(&olds[r * old.strides[i]..][..cols]);
                dst[(rows + r) * ds..(rows + r) * ds + cols].copy_from_slice(&new[i][r * f.strides[i]..][..cols]);
            }
        }
    }
    Some(buffer)
}

/// The top half of an answer onto the frame.
pub fn draw(stacked: &gst_video::VideoInfo, answer: &gst::Buffer, f: &mut Planes<'_>) {
    let Ok(frame) = gst_video::VideoFrameRef::from_buffer_ref_readable(answer.as_ref(), stacked) else { return };
    let (w, h) = (f.width as usize, f.height as usize);
    let s = stacked.stride();
    let dst = [(&mut *f.y, f.strides[0], h, w), (&mut *f.u, f.strides[1], h / 2, w / 2), (&mut *f.v, f.strides[2], h / 2, w / 2)];
    for (i, (d, ds, rows, cols)) in dst.into_iter().enumerate() {
        let Ok(src) = frame.plane_data(i as u32) else { return };
        for r in 0..rows {
            d[r * ds..r * ds + cols].copy_from_slice(&src[r * s[i] as usize..][..cols]);
        }
    }
}
