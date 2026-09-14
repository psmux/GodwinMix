//! The optional picture.
//!
//! Without `--multiview` none of this runs and the core is never asked for a
//! mosaic, which is the point: `gmx_multiview_subscribers` stays at zero and
//! the core builds no mosaic pipeline. With it, the mosaic arrives as JPEG
//! frames on the same socket and is drawn one of three ways, in this order of
//! preference:
//!
//! * kitty graphics, where the terminal answers the kitty query
//! * sixel, where the terminal's primary device attributes say it can
//! * a grid of half block characters, which every terminal can do
//!
//! Detected, never assumed. A terminal that answers neither query gets the
//! block grid, which is coarse but always right.

use crate::app::App;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Paragraph, Widget};
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picture {
    /// No picture at all: `--multiview` was not given.
    None,
    Blocks,
    Kitty,
    Sixel,
}

impl Picture {
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "blocks" | "block" => Some(Self::Blocks),
            "kitty" => Some(Self::Kitty),
            "sixel" => Some(Self::Sixel),
            "none" | "off" => Some(Self::None),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Blocks => "blocks",
            Self::Kitty => "kitty graphics",
            Self::Sixel => "sixel",
        }
    }

    /// True when the picture is painted with escape sequences after the
    /// widgets, rather than with cells inside them.
    pub fn is_escape(self) -> bool {
        matches!(self, Self::Kitty | Self::Sixel)
    }
}

/// The frame area, and the title that says what it is costing.
pub fn draw(frame: &mut Frame, app: &App, area: Rect, kind: Picture) -> Option<Rect> {
    let view = app.view();
    let title = format!(
        " mosaic {}x{} at {} fps, {} ({} frames) ",
        view.status.multiview.width,
        view.status.multiview.height,
        view.status.multiview.fps,
        kind.label(),
        app.frames_seen
    );
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(frame_data) = app.frame.as_ref() else {
        frame.render_widget(
            Paragraph::new("waiting for the first mosaic frame")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return None;
    };
    if kind == Picture::Blocks {
        if let Some(image) = decode(&frame_data.jpeg) {
            frame.render_widget(Blocks { image }, inner);
        }
        return None;
    }
    Some(inner)
}

/// A decoded mosaic frame.
pub struct Image {
    pub width: usize,
    pub height: usize,
    /// Three bytes a pixel.
    pub rgb: Vec<u8>,
}

impl Image {
    fn pixel(&self, x: usize, y: usize) -> (u8, u8, u8) {
        let i = (y.min(self.height.saturating_sub(1)) * self.width
            + x.min(self.width.saturating_sub(1)))
            * 3;
        match self.rgb.get(i..i + 3) {
            Some(p) => (p[0], p[1], p[2]),
            None => (0, 0, 0),
        }
    }

    /// Nearest neighbour, because the source is already a small mosaic and
    /// anything cleverer would cost more than the picture is worth.
    pub fn scale(&self, width: usize, height: usize) -> Self {
        let mut rgb = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            let sy = y * self.height / height.max(1);
            for x in 0..width {
                let sx = x * self.width / width.max(1);
                let (r, g, b) = self.pixel(sx, sy);
                rgb.extend_from_slice(&[r, g, b]);
            }
        }
        Self { width, height, rgb }
    }
}

#[cfg(feature = "picture")]
pub fn decode(jpeg: &[u8]) -> Option<Image> {
    let mut decoder = jpeg_decoder::Decoder::new(jpeg);
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (width, height) = (info.width as usize, info.height as usize);
    let rgb = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => pixels,
        jpeg_decoder::PixelFormat::L8 => {
            pixels.iter().flat_map(|v| [*v, *v, *v]).collect::<Vec<u8>>()
        }
        _ => return None,
    };
    Some(Image { width, height, rgb })
}

#[cfg(not(feature = "picture"))]
pub fn decode(_jpeg: &[u8]) -> Option<Image> {
    None
}

/// The mosaic as half block characters: two pixel rows per terminal row, the
/// upper half in the foreground colour and the lower half in the background.
struct Blocks {
    image: Image,
}

impl Widget for Blocks {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let scaled = self.image.scale(area.width as usize, area.height as usize * 2);
        for row in 0..area.height {
            for col in 0..area.width {
                let (tr, tg, tb) = scaled.pixel(col as usize, row as usize * 2);
                let (br, bg, bb) = scaled.pixel(col as usize, row as usize * 2 + 1);
                if let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) {
                    cell.set_char('▀')
                        .set_fg(Color::Rgb(tr, tg, tb))
                        .set_bg(Color::Rgb(br, bg, bb));
                }
            }
        }
    }
}

/// The escape sequence that paints the picture, for the two terminals that can
/// do better than characters. Cell size is guessed at 10 by 20 pixels, which
/// is close enough on every terminal tested and only affects how much of the
/// box the picture fills.
pub fn escapes(app: &App, area: Rect, kind: Picture) -> Option<String> {
    let frame = app.frame.as_ref()?;
    let image = decode(&frame.jpeg)?;
    let target_w = (area.width as usize * 10).min(image.width.max(1) * 4);
    let target_h = (area.height as usize * 20).min(image.height.max(1) * 4);
    let scaled = image.scale(target_w.max(1), target_h.max(1));
    match kind {
        Picture::Kitty => Some(kitty(&scaled, area)),
        Picture::Sixel => Some(sixel(&scaled)),
        _ => None,
    }
}

/// The kitty graphics protocol: delete what was there, then one direct
/// transmission of raw RGB, chunked at 4096 base64 characters.
fn kitty(image: &Image, area: Rect) -> String {
    let payload = base64(&image.rgb);
    let mut out = String::with_capacity(payload.len() + 256);
    out.push_str("\x1b_Ga=d,d=A\x1b\\");
    let chunks: Vec<&str> = payload.as_bytes().chunks(4096).map(|c| std::str::from_utf8(c).unwrap_or("")).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let more = usize::from(i + 1 < chunks.len());
        if i == 0 {
            out.push_str(&format!(
                "\x1b_Ga=T,f=24,s={},v={},c={},r={},q=2,m={more};{chunk}\x1b\\",
                image.width, image.height, area.width, area.height
            ));
        } else {
            out.push_str(&format!("\x1b_Gm={more};{chunk}\x1b\\"));
        }
    }
    out
}

/// Sixel, against the 216 colour cube. Six pixel rows at a time, one pass per
/// colour that appears in the band.
fn sixel(image: &Image) -> String {
    let mut out = String::from("\x1bPq\"1;1;");
    out.push_str(&format!("{};{}", image.width, image.height));
    for i in 0..216usize {
        let (r, g, b) = (i / 36, (i / 6) % 6, i % 6);
        out.push_str(&format!(
            "#{};2;{};{};{}",
            i,
            r * 100 / 5,
            g * 100 / 5,
            b * 100 / 5
        ));
    }
    let mut indexed = vec![0u8; image.width * image.height];
    for (i, px) in image.rgb.as_chunks::<3>().0.iter().enumerate() {
        indexed[i] = cube_index(px[0], px[1], px[2]);
    }
    for band in 0..image.height.div_ceil(6) {
        let mut used = [false; 216];
        for row in band * 6..((band + 1) * 6).min(image.height) {
            for x in 0..image.width {
                used[indexed[row * image.width + x] as usize] = true;
            }
        }
        for (colour, _) in used.iter().enumerate().filter(|(_, u)| **u) {
            out.push_str(&format!("#{colour}"));
            out.push_str(&sixel_band(image, &indexed, band, colour as u8));
            out.push('$');
        }
        out.push('-');
    }
    out.push_str("\x1b\\");
    out
}

/// One colour across one six pixel band, run length encoded.
fn sixel_band(image: &Image, indexed: &[u8], band: usize, colour: u8) -> String {
    let mut out = String::new();
    let mut run_char = 0u8;
    let mut run = 0usize;
    for x in 0..image.width {
        let mut bits = 0u8;
        for bit in 0..6 {
            let y = band * 6 + bit;
            if y < image.height && indexed[y * image.width + x] == colour {
                bits |= 1 << bit;
            }
        }
        if bits == run_char {
            run += 1;
        } else {
            push_run(&mut out, run_char, run);
            run_char = bits;
            run = 1;
        }
    }
    push_run(&mut out, run_char, run);
    out
}

fn push_run(out: &mut String, bits: u8, run: usize) {
    if run == 0 {
        return;
    }
    let ch = (bits + 63) as char;
    if run > 3 {
        out.push_str(&format!("!{run}{ch}"));
    } else {
        for _ in 0..run {
            out.push(ch);
        }
    }
}

fn cube_index(r: u8, g: u8, b: u8) -> u8 {
    let q = |v: u8| (v as usize * 5 / 255) as u8;
    q(r) * 36 + q(g) * 6 + q(b)
}

/// Base64, because kitty wants it and a whole crate to write forty lines would
/// be a poor trade.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_examples_everyone_knows() {
        assert_eq!(base64(b"man"), "bWFu");
        assert_eq!(base64(b"ma"), "bWE=");
        assert_eq!(base64(b"m"), "bQ==");
    }

    #[test]
    fn the_colour_cube_covers_its_corners() {
        assert_eq!(cube_index(0, 0, 0), 0);
        assert_eq!(cube_index(255, 255, 255), 215);
    }

    #[test]
    fn a_sixel_run_is_shortened_only_when_it_pays() {
        let mut out = String::new();
        push_run(&mut out, 0, 2);
        assert_eq!(out, "??");
        let mut out = String::new();
        push_run(&mut out, 63, 9);
        assert_eq!(out, "!9~");
    }

    #[test]
    fn scaling_keeps_the_corners() {
        let image = Image { width: 2, height: 2, rgb: vec![1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4] };
        let small = image.scale(1, 1);
        assert_eq!(small.rgb, vec![1, 1, 1]);
        let big = image.scale(4, 4);
        assert_eq!(big.width * big.height * 3, big.rgb.len());
    }
}
