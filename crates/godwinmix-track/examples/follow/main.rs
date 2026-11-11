//! Runs the follower over a y4m file or a folder of images and prints what it
//! would tell a camera, one line a frame. With no path it runs a made up
//! scene of a person walking across a stage.
//!
//!   cargo run --release -p godwinmix-track --example follow -- clip.y4m
//!   cargo run --release -p godwinmix-track --example follow -- frames/ --framing full
//!   cargo run --release -p godwinmix-track --features tract --example follow -- clip.y4m --tract

mod y4m;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use godwinmix_track::{
    Detector, FollowConfig, FollowOutput, Follower, FrameRef, Framing, MotionDetector, PixelFormat,
};

struct Args {
    path: Option<PathBuf>,
    framing: Framing,
    every: u32,
    tract: bool,
}

fn args() -> Result<Args, String> {
    let mut a = Args { path: None, framing: Framing::Waist, every: 1, tract: false };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--framing" => {
                let v = it.next().unwrap_or_default();
                a.framing = Framing::from_slug(&v).ok_or(format!("--framing takes close, waist, full or wide, not {v:?}"))?;
            }
            "--every" => a.every = it.next().and_then(|v| v.parse().ok()).ok_or("--every takes a number")?,
            "--tract" => a.tract = true,
            p => a.path = Some(PathBuf::from(p)),
        }
    }
    Ok(a)
}

fn detector(tract: bool) -> Result<Box<dyn Detector + Send>, String> {
    if !tract {
        return Ok(Box::new(MotionDetector::default()));
    }
    #[cfg(feature = "tract")]
    {
        use godwinmix_track::detect::tract::{TractConfig, TractDetector};
        let d = TractDetector::new(TractConfig { input: (320, 192), ..TractConfig::default() });
        return d.map(|d| Box::new(d) as Box<dyn Detector + Send>).map_err(|e| e.to_string());
    }
    #[allow(unreachable_code)]
    Err("--tract needs the example built with --features tract".into())
}

fn print(n: usize, out: &FollowOutput, took: Duration) {
    let target = out.target.map_or("nobody".to_string(), |id| id.to_string());
    let (p, c) = (out.ptz, out.crop);
    println!(
        "frame {n:4} {} tracks {} {target:>9} {:<10} pan {:+.2} tilt {:+.2} zoom {:+.2}  crop {:.2} {:.2} {:.2}x{:.2}  {:.2} ms",
        if out.detected { "detect" } else { "coast " },
        out.tracks.len(),
        format!("{:?}", out.state).to_lowercase(),
        p.pan,
        p.tilt,
        p.zoom,
        c.x,
        c.y,
        c.w,
        c.h,
        took.as_secs_f64() * 1000.0
    );
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let a = args()?;
    let cfg = FollowConfig { detect_every: a.every, ..FollowConfig::default() };
    let mut f = Follower::new(detector(a.tract)?, cfg);
    f.set_framing(a.framing);
    let (t0, mut out) = (Instant::now(), FollowOutput::default());
    let mut step = |n: usize, fps: f32, frame: &FrameRef| {
        let start = Instant::now();
        f.step_into(frame, t0 + Duration::from_secs_f32(n as f32 / fps), &mut out);
        print(n, &out, start.elapsed());
    };
    match a.path {
        None => demo(&mut step),
        Some(p) if p.is_dir() => images(&p, &mut step),
        Some(p) => video(&p, &mut step),
    }
}

fn video(path: &Path, step: &mut impl FnMut(usize, f32, &FrameRef)) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("could not open {}: {e}", path.display()))?;
    let mut y = y4m::Y4m::open(file)?;
    let (mut buf, mut n) = (Vec::new(), 0);
    while y.next_frame(&mut buf)? {
        let frame = FrameRef::packed(y.width, y.height, y.format, &buf).map_err(|e| e.to_string())?;
        step(n, y.fps, &frame);
        n += 1;
    }
    Ok(())
}

fn images(dir: &Path, step: &mut impl FnMut(usize, f32, &FrameRef)) -> Result<(), String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("could not list {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| ["jpg", "jpeg", "png"].contains(&x.to_string_lossy().to_lowercase().as_str())))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(format!("no .jpg or .png files in {}", dir.display()));
    }
    for (n, p) in files.iter().enumerate() {
        let img = image::open(p).map_err(|e| format!("{}: {e}", p.display()))?.to_rgb8();
        let (w, h) = img.dimensions();
        let frame = FrameRef::packed(w, h, PixelFormat::Rgb8, img.as_raw()).map_err(|e| e.to_string())?;
        step(n, 10.0, &frame);
    }
    Ok(())
}

/// A dark figure in striped clothes walks right across a grey stage.
fn demo(step: &mut impl FnMut(usize, f32, &FrameRef)) -> Result<(), String> {
    let (w, h) = (320u32, 180u32);
    let mut px = vec![0u8; (w * h) as usize];
    for n in 0..150 {
        let x0 = 30 + n as u32;
        for y in 0..h {
            for x in 0..w {
                let person = (x0..x0 + 24).contains(&x) && (40..160).contains(&y);
                let stripe = (x - x0.min(x)) / 3 % 2 == 0;
                px[(y * w + x) as usize] = if person { if stripe { 40 } else { 70 } } else { 150 + ((x + y) % 7) as u8 };
            }
        }
        let frame = FrameRef::packed(w, h, PixelFormat::Gray8, &px).map_err(|e| e.to_string())?;
        step(n, 25.0, &frame);
    }
    Ok(())
}
