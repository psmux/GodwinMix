//! MJPEG: `multipart/x-mixed-replace`, one JPEG per part, fed from the mosaic.
//!
//! # Why the mosaic and not a second encoder
//!
//! The mosaic already encodes one JPEG per frame for every cell at once. A
//! separate encoder per viewer would cost one encode per client per camera; a
//! crop out of the mosaic costs one decode, one crop and one encode, and the
//! whole sheet costs nothing at all because those bytes are the mosaic's own.
//! Six people watching six different cameras therefore share one mosaic encoder
//! rather than running six.
//!
//! The consequence is that a cell is mosaic sized, not camera sized. For
//! picking a camera, seeing who is in shot, or drawing designer handles, that
//! is what is wanted. A client that needs more asks for a wider mosaic through
//! `?width=`, which is one number on one pipeline for everybody.
//!
//! # What holds it up
//!
//! Every stream holds a `MultiviewSubscription` for its whole life, so opening
//! one builds the mosaic if it is not there and closing the last one takes it
//! away after the linger. There is no path to these bytes that does not also
//! pay for them.

use crate::snapshot::{self, Pick};
use crate::state::CellAssignment;
use anyhow::Result;

/// The boundary between parts. Any token works as long as the header and the
/// body agree; this one is what `curl` and every MJPEG client in the wild has
/// seen a thousand times.
pub const BOUNDARY: &str = "gmxframe";

/// The `Content-Type` a client must be sent for the parts below to be read as
/// a stream rather than as one file.
pub fn content_type() -> String {
    format!("multipart/x-mixed-replace; boundary={BOUNDARY}")
}

/// One part: the boundary, the headers, the JPEG, and the blank line after it.
///
/// `Content-Length` is there because without it a client has to scan for the
/// next boundary, and a JPEG can contain the boundary bytes by chance.
pub fn part(jpeg: &[u8]) -> Vec<u8> {
    let head = format!(
        "--{BOUNDARY}\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
        jpeg.len()
    );
    let mut out = Vec::with_capacity(head.len() + jpeg.len() + 2);
    out.extend_from_slice(head.as_bytes());
    out.extend_from_slice(jpeg);
    out.extend_from_slice(b"\r\n");
    out
}

/// What one MJPEG stream is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The whole mosaic.
    Sheet,
    /// The programme return cell.
    Program,
    /// One source's cell.
    Source(String),
    /// The armed scene. Until the scene server lands this is the programme
    /// tile, which is what the operator is looking at anyway when nothing is
    /// armed.
    Preview,
    /// One scene item as a projector. Needs the scene server.
    Item(String),
}

impl Target {
    /// Parse the last path segment of `/mjpeg/...`.
    pub fn parse(segment: &str) -> Self {
        match segment {
            "sheet" => Self::Sheet,
            "program" | "programme" => Self::Program,
            "preview" => Self::Preview,
            id => Self::Source(id.to_string()),
        }
    }

    /// The pick on the mosaic this target reads, or `None` when it is not a
    /// cell at all.
    pub fn pick(&self) -> Option<Pick> {
        match self {
            Self::Sheet => Some(Pick::Sheet),
            // With no armed scene the preview is the programme tile, and
            // `event/multiview.layout` says the preview is empty.
            Self::Program | Self::Preview => Some(Pick::Program),
            Self::Source(id) => Some(Pick::Source(id.clone())),
            Self::Item(_) => None,
        }
    }

    pub fn is_sheet(&self) -> bool {
        matches!(self, Self::Sheet)
    }
}

/// Turn one mosaic frame into what this stream sends.
///
/// Blocking: it decodes and encodes a JPEG. Every caller runs it on a blocking
/// thread, never on an async worker.
///
/// A whole sheet at its natural size is handed back untouched, which is the
/// common case for a designer or a Tkinter window and costs nothing.
pub fn cut(
    mosaic_jpeg: &[u8],
    cell: Option<&CellAssignment>,
    width: Option<u32>,
) -> Result<Vec<u8>, image::ImageError> {
    if cell.is_none() && width.is_none() {
        return Ok(mosaic_jpeg.to_vec());
    }
    let decoded = snapshot::decode_jpeg(mosaic_jpeg)?;
    let img = match cell {
        Some(c) => snapshot::crop_cell(&decoded, c),
        None => decoded,
    };
    snapshot::encode_jpeg(&snapshot::fit_width(img, width))
}

/// What to tell a client that asked for a cell the mosaic does not have.
pub fn no_such_cell(target: &Target, cells: &[CellAssignment]) -> String {
    let on_sheet: Vec<String> = cells
        .iter()
        .map(|c| c.source.clone().unwrap_or_else(|| "program".into()))
        .collect();
    let name = match target {
        Target::Source(id) => id.clone(),
        other => format!("{other:?}").to_lowercase(),
    };
    format!(
        "no cell for '{name}' on the mosaic. On it now: {}. Add the source, or ask for \
         /mjpeg/sheet.",
        if on_sheet.is_empty() { "nothing".to_string() } else { on_sheet.join(", ") }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(index: u32, source: Option<&str>, x: i32, y: i32, w: i32, h: i32) -> CellAssignment {
        CellAssignment { index, source: source.map(String::from), x, y, w, h }
    }

    #[test]
    fn a_target_reads_the_names_a_client_types() {
        assert_eq!(Target::parse("sheet"), Target::Sheet);
        assert_eq!(Target::parse("program"), Target::Program);
        assert_eq!(Target::parse("preview"), Target::Preview);
        assert_eq!(Target::parse("cam1"), Target::Source("cam1".into()));
        assert_eq!(Target::parse("cam1").pick(), Some(Pick::Source("cam1".into())));
        assert_eq!(Target::Item("x".into()).pick(), None);
    }

    #[test]
    fn a_part_declares_its_length_so_a_client_never_scans_for_a_boundary() {
        let jpeg = [0xFFu8, 0xD8, 0x11, 0x22, 0xFF, 0xD9];
        let bytes = part(&jpeg);
        let text = String::from_utf8_lossy(&bytes[..60]);
        assert!(text.starts_with("--gmxframe\r\n"), "{text}");
        assert!(text.contains("Content-Type: image/jpeg"));
        assert!(text.contains("Content-Length: 6"));
        assert!(bytes.ends_with(b"\r\n"));
        assert!(content_type().contains(BOUNDARY));
    }

    #[test]
    fn a_whole_sheet_at_its_own_size_is_passed_through_untouched() {
        let jpeg = b"not really a jpeg".to_vec();
        assert_eq!(cut(&jpeg, None, None).unwrap(), jpeg, "the cheap path must not re-encode");
    }

    #[test]
    fn a_cell_comes_out_the_size_of_the_cell() {
        // A real two by two mosaic, encoded so the cut has something to decode.
        let mut img = image::RgbImage::new(64, 64);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb([if x < 32 { 255 } else { 0 }, if y < 32 { 255 } else { 0 }, 0]);
        }
        let sheet = snapshot::encode_jpeg(&img).unwrap();
        let c = cell(0, Some("cam1"), 32, 0, 32, 32);
        let out = cut(&sheet, Some(&c), None).unwrap();
        let decoded = snapshot::decode_jpeg(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (32, 32));
        // And narrowing it narrows it.
        let narrow = cut(&sheet, Some(&c), Some(16)).unwrap();
        assert_eq!(snapshot::decode_jpeg(&narrow).unwrap().width(), 16);
    }

    #[test]
    fn a_missing_cell_says_what_is_on_the_sheet() {
        let cells = vec![cell(0, None, 0, 0, 10, 10), cell(1, Some("cam1"), 10, 0, 10, 10)];
        let message = no_such_cell(&Target::Source("cam9".into()), &cells);
        assert!(message.contains("cam9"), "{message}");
        assert!(message.contains("program, cam1"), "{message}");
        assert!(message.contains("/mjpeg/sheet"), "the message must name the next step");
    }
}
