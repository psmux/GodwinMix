//! A small running mixer and a way to read its programme frames back.
//!
//! Shared by three test files, each of which uses only some of it.
#![allow(dead_code)]

use super::*;

/// The last programme frame, and the intervals between frames.
#[derive(Clone, Default)]
pub struct Frames {
    last: Arc<Mutex<Option<gst::Buffer>>>,
    pub intervals: Arc<Mutex<Vec<f64>>>,
}

/// One I420 frame of the 320x180 canvas.
pub struct Frame(gst::Buffer);

impl Frame {
    /// The Y, U and V at a pixel.
    pub fn yuv(&self, x: usize, y: usize) -> (u8, u8, u8) {
        let map = self.0.map_readable().unwrap();
        let (w, h) = (320usize, 180usize);
        let luma = map[y * w + x];
        let u = map[w * h + (y / 2) * (w / 2) + x / 2];
        let v = map[w * h + (w / 2) * (h / 2) + (y / 2) * (w / 2) + x / 2];
        (luma, u, v)
    }
}

impl Frames {
    pub fn latest(&self) -> Option<Frame> {
        self.last.lock().unwrap().clone().map(Frame)
    }

    pub fn worst_interval(&self) -> f64 {
        self.intervals.lock().unwrap().iter().cloned().fold(0.0, f64::max)
    }
}

/// A mixer on a 320x180 canvas at 30 fps, with every programme frame seen.
pub fn running() -> (MixerHandle, Frames, std::thread::JoinHandle<()>) {
    gst::init().unwrap();
    let cfg: Config = toml::from_str(
        "[canvas]\nwidth = 320\nheight = 180\nfps = 30\nsample_rate = 48000\nchannels = 2\n\n[control]\nbind = \"127.0.0.1:0\"\n",
    )
    .unwrap();
    let (mut mix, handle, cmd_rx, _bus_rx) = Mixer::build(cfg).expect("build a mixer");
    let frames = Frames::default();
    let tee = mix.program_pipeline().by_name("vraw-tee").expect("the raw tee");
    let pad = tee.static_pad("sink").unwrap();
    let seen = frames.clone();
    let then: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        if let Some(gst::PadProbeData::Buffer(b)) = &info.data {
            *seen.last.lock().unwrap() = Some(b.copy_deep().unwrap());
        }
        let now = std::time::Instant::now();
        if let Some(t) = then.lock().unwrap().replace(now) {
            seen.intervals.lock().unwrap().push((now - t).as_secs_f64() * 1000.0);
        }
        gst::PadProbeReturn::Ok
    });
    mix.start().expect("start the mixer");
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    (handle, frames, thread)
}

pub async fn add(handle: &MixerHandle, cfg: SourceConfig) {
    handle.request(|ack| Command::AddSource(Box::new(cfg), Some(ack))).await.expect("add a source");
}

pub fn full(id: &str) -> Placement {
    at(id, 0, 0, 320, 180)
}

pub fn at(id: &str, x: i32, y: i32, w: i32, h: i32) -> Placement {
    let canvas = godwinmix_core::caps::CanvasCaps { width: 320, height: 180, fps: gst::Fraction::new(30, 1), sample_rate: 48000, channels: 2 };
    Placement { xpos: x, ypos: y, width: w, height: h, sizing: Sizing::Fill, ..Placement::full_canvas(id.into(), &canvas) }
}

pub async fn take(handle: &MixerHandle, placements: Vec<Placement>) {
    let scene = ProgramScene { name: "test".into(), placements };
    handle
        .request(|ack| Command::TakeScene {
            scene: Box::new(scene),
            at_running_time_ms: None,
            duration_ms: None,
            transition: None,
            ack: Some(ack),
        })
        .await
        .expect("take the scene");
}

pub async fn settle(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

pub fn stop(handle: MixerHandle, thread: std::thread::JoinHandle<()>) {
    handle.send(Command::Shutdown).ok();
    let _ = thread.join();
}

/// Write a PNG whose left half is opaque red and right half fully clear.
pub fn half_red_png(path: &std::path::Path, w: u32, h: u32) {
    gst::init().unwrap();
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..h {
        for x in 0..w {
            rgba.extend(if x < w / 2 { [255, 0, 0, 255] } else { [0, 0, 0, 0] });
        }
    }
    let pipe = gst::parse::launch(&format!(
        "appsrc name=s caps=video/x-raw,format=RGBA,width={w},height={h},framerate=1/1 ! pngenc ! filesink location=\"{}\"",
        path.display()
    ))
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let src = pipe.by_name("s").unwrap().downcast::<gstreamer_app::AppSrc>().unwrap();
    pipe.set_state(gst::State::Playing).unwrap();
    src.push_buffer(gst::Buffer::from_mut_slice(rgba)).unwrap();
    src.end_of_stream().unwrap();
    pipe.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(5), &[gst::MessageType::Eos]);
    pipe.set_state(gst::State::Null).unwrap();
}

/// A scratch directory for one test.
pub fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-overlay-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Within a few steps of each other on every component.
pub fn near(a: (u8, u8, u8), b: (u8, u8, u8)) -> bool {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs() <= 6;
    d(a.0, b.0) && d(a.1, b.1) && d(a.2, b.2)
}
