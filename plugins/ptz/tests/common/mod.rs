//! A fake mixer for the plugin's tests: it serves `/api/v1/snapshot/<id>`
//! as one JPEG and `/mjpeg/<id>` as a multipart stream of a recorded scene,
//! a bright upright box (a person) walking across a dark stage, at 15
//! pictures a second. Real HTTP on loopback, real JPEGs.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, RgbImage};

pub const W: u32 = 320;
pub const H: u32 = 180;

/// Where the person stands in picture `i`: their box's left edge, 0 to 1.
/// They walk from just right of centre to the right and back.
pub fn person_x(i: usize) -> f32 {
    let t = (i % 120) as f32 / 120.0;
    0.55 + 0.25 * (1.0 - (2.0 * t - 1.0).abs())
}

pub fn picture(i: usize) -> Vec<u8> {
    let mut img = RgbImage::from_pixel(W, H, image::Rgb([20, 22, 30]));
    let (x0, y0) = ((person_x(i) * W as f32) as u32, (0.25 * H as f32) as u32);
    let (pw, ph) = ((0.08 * W as f32) as u32, (0.6 * H as f32) as u32);
    for y in y0..(y0 + ph).min(H) {
        for x in x0..(x0 + pw).min(W) {
            img.put_pixel(x, y, image::Rgb([220, 180, 150]));
        }
    }
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 80).encode(img.as_raw(), W, H, ExtendedColorType::Rgb8).unwrap();
    out
}

/// The mixer on loopback. Answers `http://127.0.0.1:port`.
pub fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let frames: Vec<Vec<u8>> = (0..120).map(picture).collect();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let frames = frames.clone();
            std::thread::spawn(move || {
                let mut conn = conn;
                // Read the whole request first: closing with any of it
                // unread makes Linux reset the connection, and the reset can
                // beat the answer to the client.
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match conn.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let request = String::from_utf8_lossy(&buf).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
                if path.starts_with("/api/v1/snapshot/pulpit") {
                    let _ = write!(conn, "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", frames[0].len());
                    let _ = conn.write_all(&frames[0]);
                } else if path.starts_with("/mjpeg/pulpit") {
                    let _ = write!(conn, "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\n\r\n");
                    for i in 0.. {
                        let f = &frames[i % frames.len()];
                        let head = format!("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", f.len());
                        if conn.write_all(head.as_bytes()).and_then(|_| conn.write_all(f)).and_then(|_| conn.write_all(b"\r\n")).is_err() {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(66));
                    }
                } else {
                    let body = "{\"error\":\"no such source\"}";
                    let _ = write!(conn, "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                }
            });
        }
    });
    base
}
