//! Multiview frames off the wire.
//!
//! A binary frame on `/rpc` is a 16 byte header then JPEG:
//!
//! ```text
//! offset 0   u32  seq                little endian
//! offset 4   u32  layout id          little endian
//! offset 8   u64  running time ms    little endian
//! offset 16  ...  JPEG bytes
//! ```
//!
//! The layout id matches the id in `event/multiview.layout`, which is what
//! lets a client cut cells out of a sheet without a race when the layout
//! changes mid flight. The core's writer is `api::rpc::frame_header`, and the
//! test at the bottom of this file checks this reader against the bytes that
//! writer produces.

pub const HEADER_BYTES: usize = 16;

/// One mosaic frame, header read and JPEG still compressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Counts up per connection. Wraps rather than stopping.
    pub seq: u32,
    /// Which `event/multiview.layout` this picture was cut for.
    pub layout: u32,
    /// Programme running time the frame was taken at, in milliseconds.
    pub running_time_ms: u64,
    pub jpeg: Vec<u8>,
}

impl Frame {
    /// The cell rectangle for one source, from the layout with this frame's id.
    ///
    /// Returns None when the layout is a different one, which is the case a
    /// client hits for a frame or two after the grid changes.
    pub fn cell<'a>(
        &self,
        layout: &'a crate::MultiviewLayout,
        source: &str,
    ) -> Option<&'a crate::CellAssignment> {
        if layout.id != self.layout {
            return None;
        }
        layout.cells.iter().find(|c| c.source.as_deref() == Some(source))
    }
}

/// Split one binary message into its header and its JPEG.
///
/// Returns None for anything too short to be a frame, which is what a client
/// gets if it points at a core that sends bare JPEGs.
pub fn parse_frame(bytes: &[u8]) -> Option<Frame> {
    if bytes.len() <= HEADER_BYTES {
        return None;
    }
    let seq = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let layout = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let running_time_ms = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
    Some(Frame { seq, layout, running_time_ms, jpeg: bytes[HEADER_BYTES..].to_vec() })
}

/// The mosaic width to ask the core for.
///
/// The sheet holds `cols` cells across. A tile that is `tile_px` real pixels
/// wide wants `tile_px * cols`, rounded up to a multiple of 16 because
/// encoders like even macroblocks, and clamped to what the protocol allows.
/// Asking for more than this is bytes nobody looks at.
pub fn sheet_width_for(tile_px: u32, cols: u32) -> u32 {
    let sheet = tile_px.max(1) * cols.max(1);
    let rounded = sheet.div_ceil(16) * 16;
    rounded.clamp(320, 1920)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same bytes `api::rpc::frame_header` writes, built here by hand so
    /// this is a test of the pair and not a restatement of one side.
    fn header(seq: u32, layout: u32, ms: u64) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES);
        out.extend_from_slice(&seq.to_le_bytes());
        out.extend_from_slice(&layout.to_le_bytes());
        out.extend_from_slice(&ms.to_le_bytes());
        out
    }

    #[test]
    fn reads_a_frame() {
        let mut bytes = header(7, 3, 1234);
        bytes.extend_from_slice(&[0xff, 0xd8, 0xff, 0xe0]);
        let frame = parse_frame(&bytes).expect("a 20 byte message is a frame");
        assert_eq!(frame.seq, 7);
        assert_eq!(frame.layout, 3);
        assert_eq!(frame.running_time_ms, 1234);
        assert_eq!(frame.jpeg, vec![0xff, 0xd8, 0xff, 0xe0]);
    }

    #[test]
    fn a_header_with_no_picture_is_not_a_frame() {
        assert!(parse_frame(&header(1, 1, 1)).is_none());
        assert!(parse_frame(&[]).is_none());
    }

    #[test]
    fn widths_are_rounded_and_clamped() {
        assert_eq!(sheet_width_for(100, 3), 304_u32.max(320));
        assert_eq!(sheet_width_for(320, 3), 960);
        assert_eq!(sheet_width_for(1000, 4), 1920);
    }
}
