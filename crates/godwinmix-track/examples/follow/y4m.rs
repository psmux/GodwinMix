//! Just enough of the YUV4MPEG2 format to read 4:2:0 and grey frames, as
//! `ffmpeg -i in.mp4 -vf scale=320:-2 -pix_fmt yuv420p out.y4m` writes them.

use std::io::{BufRead, BufReader, Read};

use godwinmix_track::PixelFormat;

pub struct Y4m<R> {
    reader: BufReader<R>,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub format: PixelFormat,
    line: String,
}

impl<R: Read> Y4m<R> {
    pub fn open(inner: R) -> Result<Self, String> {
        let mut reader = BufReader::new(inner);
        let mut header = String::new();
        reader.read_line(&mut header).map_err(|e| e.to_string())?;
        let mut words = header.split_whitespace();
        if words.next() != Some("YUV4MPEG2") {
            return Err("not a y4m file: it does not start with YUV4MPEG2".into());
        }
        let (mut width, mut height, mut fps, mut format) = (0, 0, 25.0, PixelFormat::I420);
        for w in words {
            let (tag, val) = w.split_at(1);
            match tag {
                "W" => width = val.parse().unwrap_or(0),
                "H" => height = val.parse().unwrap_or(0),
                "F" => fps = parse_rate(val).unwrap_or(25.0),
                "C" if val.starts_with("mono") => format = PixelFormat::Gray8,
                "C" if !val.starts_with("420") => {
                    return Err(format!("y4m colour space {val} is not supported; convert with -pix_fmt yuv420p"));
                }
                _ => {}
            }
        }
        if width == 0 || height == 0 {
            return Err("the y4m header has no W and H".into());
        }
        Ok(Self { reader, width, height, fps, format, line: String::new() })
    }

    /// Reads the next frame into `buf`. False at the end of the file.
    pub fn next_frame(&mut self, buf: &mut Vec<u8>) -> Result<bool, String> {
        self.line.clear();
        let n = self.reader.read_line(&mut self.line).map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(false);
        }
        if !self.line.starts_with("FRAME") {
            return Err(format!("expected FRAME, found {:?}", self.line.trim()));
        }
        let len = self.format.frame_len(self.format.min_stride(self.width), self.height);
        buf.resize(len, 0);
        self.reader.read_exact(buf).map_err(|e| e.to_string())?;
        Ok(true)
    }
}

fn parse_rate(v: &str) -> Option<f32> {
    let (n, d) = v.split_once(':')?;
    Some(n.parse::<f32>().ok()? / d.parse::<f32>().ok()?.max(1.0))
}
