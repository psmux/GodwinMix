//! The programme's running time written into a picture and read back out.
//!
//! An 8 by 4 grid of luma blocks: the first 26 are the bits of the running
//! time in milliseconds, lowest first, and the last six are a fixed pattern a
//! reader checks before it believes the rest. Blocks that size survive the
//! thumbnail scale, the mosaic's own scale and its JPEG.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;

const BITS: u32 = 26;
const COLS: u32 = 8;
const ROWS: u32 = 4;
const MARK: [u8; 6] = [1, 0, 1, 0, 1, 0];

/// The running time of `pipeline` now, in milliseconds.
pub fn running_now(pipeline: &gst::Pipeline) -> Option<i64> {
    let now = pipeline.clock()?.time().checked_sub(pipeline.base_time()?)?;
    Some(now.mseconds() as i64)
}

/// Write `ms` into the luma plane of `buffer`, a block per bit.
fn stamp(buffer: &mut gst::BufferRef, info: &gst_video::VideoInfo, ms: i64) {
    let Ok(mut frame) = gst_video::VideoFrameRef::from_buffer_ref_writable(buffer, info) else { return };
    let (w, h) = (info.width(), info.height());
    let stride = info.stride()[0] as usize;
    let Ok(luma) = frame.plane_data_mut(0) else { return };
    for y in 0..h {
        for x in 0..w {
            let block = (y * ROWS / h) * COLS + x * COLS / w;
            let bit = match block {
                b if b < BITS => (ms >> b) & 1 == 1,
                b => MARK[(b - BITS) as usize] == 1,
            };
            luma[y as usize * stride + x as usize] = if bit { 235 } else { 16 };
        }
    }
}

/// Stamp every frame as it leaves the normaliser of `source`, the tee that
/// feeds the programme and the mosaic alike, with the running time it left at.
pub fn stamp_frames_at(source: &gst::Pipeline, tee: &str) {
    let tee = source.by_name(tee).expect("the source's video tee");
    let pad = tee.static_pad("sink").unwrap();
    let pipeline = source.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let video = pad.current_caps().and_then(|c| gst_video::VideoInfo::from_caps(&c).ok());
        if let (Some(video), Some(ms), Some(buffer)) = (video, running_now(&pipeline), info.buffer_mut()) {
            stamp(buffer.make_mut(), &video, ms);
        }
        gst::PadProbeReturn::Ok
    });
}

/// The stamp in a picture, read from the middle of each block.
pub fn read(img: &image::RgbImage) -> Option<i64> {
    let (w, h) = img.dimensions();
    let mut bits = Vec::new();
    for r in 0..ROWS {
        for c in 0..COLS {
            let (x, y) = ((2 * c + 1) * w / (2 * COLS), (2 * r + 1) * h / (2 * ROWS));
            let p = img.get_pixel(x, y).0;
            bits.push(u8::from((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3 > 128));
        }
    }
    (bits[BITS as usize..] == MARK).then(|| (0..BITS).map(|i| (bits[i as usize] as i64) << i).sum())
}

/// One cell of a mosaic picture.
pub fn cell_of(img: &image::RgbImage, cell: &crate::state::CellAssignment) -> image::RgbImage {
    image::imageops::crop_imm(img, cell.x as u32, cell.y as u32, cell.w as u32, cell.h as u32).to_image()
}
