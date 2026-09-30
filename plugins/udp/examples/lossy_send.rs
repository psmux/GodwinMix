//! Send an MPEG-TS file over UDP at its own bitrate, losing some on purpose.
//!
//! ```sh
//! cargo run -p gmx-udp --example lossy_send -- clip.ts 239.1.1.1 19471 --loss 1 --seconds 30
//! cargo run -p gmx-udp --example lossy_send -- clip.ts 239.1.1.1 19471 --rtp --loss 1
//! ```
//!
//! Seven packets to a datagram, paced by the PCR free rule of thumb that the
//! file's size over its duration is its rate (so give it a constant bitrate
//! file; ffmpeg -muxrate makes one). `--loss` is a percentage of datagrams
//! dropped at random before they reach the socket, which is what a congested
//! switch port does. `--pause` stops sending for that many seconds half way,
//! for the feed that goes away and comes back. The duration comes from
//! `ffprobe`.

use std::net::UdpSocket;
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CHUNK: usize = 7 * 188;

struct Args {
    file: String,
    host: String,
    port: u16,
    loss: f64,
    seconds: f64,
    rtp: bool,
    pause: f64,
}

fn parse(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut plain = Vec::new();
    let mut a = Args { file: String::new(), host: String::new(), port: 0, loss: 0.0, seconds: 30.0, rtp: false, pause: 0.0 };
    while let Some(arg) = it.next() {
        let mut num = |name: &str| -> Result<f64, String> {
            it.next().and_then(|v| v.parse().ok()).ok_or(format!("{name} takes a number"))
        };
        match arg.as_str() {
            "--loss" => a.loss = num("--loss")?,
            "--seconds" => a.seconds = num("--seconds")?,
            "--pause" => a.pause = num("--pause")?,
            "--rtp" => a.rtp = true,
            _ => plain.push(arg),
        }
    }
    let [file, host, port] = <[String; 3]>::try_from(plain).map_err(|_| "usage: lossy_send FILE HOST PORT [--loss %] [--seconds s] [--rtp] [--pause s]".to_string())?;
    a.port = port.parse().map_err(|_| format!("the port must be a number, not {port}"))?;
    (a.file, a.host) = (file, host);
    Ok(a)
}

fn duration(path: &str) -> Result<f64, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path])
        .output()
        .map_err(|e| format!("could not run ffprobe: {e}"))?;
    String::from_utf8_lossy(&out.stdout).trim().parse().map_err(|_| format!("ffprobe gave no duration for {path}"))
}

/// A small xorshift, seeded from the clock: loss only has to look random.
struct Dice(u64);

impl Dice {
    fn percent(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64 * 100.0
    }
}

/// An RTP header: version 2, payload type 33 (MP2T), the sequence number, a
/// 90 kHz timestamp and a fixed SSRC.
fn rtp_header(seq: u16, ts: u32) -> [u8; 12] {
    let mut h = [0u8; 12];
    h[0] = 0x80;
    h[1] = 33;
    h[2..4].copy_from_slice(&seq.to_be_bytes());
    h[4..8].copy_from_slice(&ts.to_be_bytes());
    h[8..12].copy_from_slice(&0x6D6978u32.to_be_bytes());
    h
}

/// Send `data` at `rate` bytes a second, as the arguments say. Returns the
/// datagrams sent and dropped.
fn send(a: &Args, data: &[u8], rate: f64, sock: &UdpSocket) -> std::io::Result<(u64, u64)> {
    let (mut sent, mut dropped, mut seq, mut offset) = (0u64, 0u64, 0u16, 0usize);
    let mut dice = Dice(SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1);
    let mut t0 = Instant::now();
    let mut paused = false;
    while t0.elapsed().as_secs_f64() < a.seconds {
        if a.pause > 0.0 && !paused && t0.elapsed().as_secs_f64() > a.seconds / 2.0 {
            std::thread::sleep(Duration::from_secs_f64(a.pause));
            paused = true;
            t0 += Duration::from_secs_f64(a.pause);
        }
        let piece = &data[offset..(offset + CHUNK).min(data.len())];
        offset = if offset + CHUNK < data.len() { offset + CHUNK } else { 0 };
        seq = seq.wrapping_add(1);
        if dice.percent() < a.loss {
            dropped += 1;
        } else {
            let mut out = Vec::with_capacity(12 + piece.len());
            if a.rtp {
                let ts = (SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64()) * 90000.0) as u64;
                out.extend_from_slice(&rtp_header(seq, ts as u32));
            }
            out.extend_from_slice(piece);
            sock.send_to(&out, (a.host.as_str(), a.port))?;
            sent += 1;
        }
        // Pace to the file's rate: where the stream should be by now.
        let ahead = (sent + dropped) as f64 * CHUNK as f64 / rate - t0.elapsed().as_secs_f64();
        if ahead > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(ahead));
        }
    }
    Ok((sent, dropped))
}

fn run() -> Result<(), String> {
    let a = parse(std::env::args().skip(1))?;
    let data = std::fs::read(&a.file).map_err(|e| format!("could not read {}: {e}", a.file))?;
    let rate = data.len() as f64 / duration(&a.file)?;
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    let _ = sock.set_multicast_ttl_v4(1);
    let _ = sock.set_multicast_loop_v4(true);
    let (sent, dropped) = send(&a, &data, rate, &sock).map_err(|e| format!("sending failed: {e}"))?;
    let share = 100.0 * dropped as f64 / (sent + dropped).max(1) as f64;
    println!("sent {sent} datagrams, dropped {dropped} on purpose ({share:.2}%)");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "lossy_send/tests.rs"]
mod tests;
