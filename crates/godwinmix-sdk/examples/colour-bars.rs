//! A complete source plugin: eight colour bars at canvas caps.
//!
//! The worked plugin from the plugin architecture appendix, in Rust on this
//! crate. Change `draw_bars` and the settings schema and you have a different
//! source; nothing else here needs touching. `examples/test-zero-dep.sh` in the
//! repository root shows how to drive a plugin like this by hand, with a
//! handshake on stdin and the Matroska stream landing in a file.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use godwinmix_sdk::prelude::*;
use serde_json::Value;

/// Y, U and V for the eight standard bars, white through to black.
const BARS: [[u8; 3]; 8] = [
    [235, 128, 128],
    [210, 16, 146],
    [170, 166, 16],
    [145, 54, 34],
    [106, 202, 222],
    [81, 90, 240],
    [41, 240, 110],
    [16, 128, 128],
];

struct ColourBars {
    canvas: Canvas,
    /// Thousandths of a pixel per second the bars slide sideways, 0 to hold
    /// still. Shared with the media thread so `configure` lands on the next
    /// frame without stopping anything.
    drift: Arc<AtomicI64>,
    media: Option<VideoLoop>,
    reporter: Option<Reporter>,
}

/// Fill one I420 frame with the bars, offset by `shift` pixels.
///
/// One row of each plane is computed and copied down: the bars do not change
/// with the row, so drawing 1,080 of them would be 1,080 times the work.
fn draw_bars(frame: &mut [u8], canvas: Canvas, shift: i64) {
    let width = canvas.width as usize;
    let height = canvas.height as usize;
    let cw = width.div_ceil(2);
    let ch = height.div_ceil(2);
    let (luma, chroma) = frame.split_at_mut(width * height);
    let (u, v) = chroma.split_at_mut(cw * ch);
    let bar_of = |x: usize| {
        let bar_width = (width as i64 / 8).max(1);
        let shifted = (x as i64 - shift).rem_euclid(width.max(1) as i64);
        BARS[((shifted / bar_width) as usize).min(7)]
    };
    for (x, pixel) in luma.iter_mut().take(width).enumerate() {
        *pixel = bar_of(x)[0];
    }
    for (x, (u, v)) in u.iter_mut().zip(v.iter_mut()).take(cw).enumerate() {
        *u = bar_of(x * 2)[1];
        *v = bar_of(x * 2)[2];
    }
    for row in 1..height {
        luma.copy_within(0..width, row * width);
    }
    for row in 1..ch {
        u.copy_within(0..cw, row * cw);
        v.copy_within(0..cw, row * cw);
    }
}

fn drift_from(params: &Value) -> i64 {
    let px = params
        .get("drift_px_per_sec")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    (px * 1000.0) as i64
}

impl Source for ColourBars {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        self.canvas = ready.canvas;
        self.drift.store(drift_from(&ready.params), Ordering::Relaxed);
        reporter.info(format!(
            "colour bars at {}x{}@{} for instance '{}'",
            ready.canvas.width, ready.canvas.height, ready.canvas.fps, ready.instance
        ));
        self.reporter = Some(reporter);
        // Bars are drawn on demand, so nothing is buffered and nothing is late.
        Ok(InitializeResult { latency_ms: Some(0) })
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        let canvas = params.canvas;
        let writer = media::open(
            params.transport,
            &params.media,
            canvas,
            media::Streams::video_only(media::VideoFormat::I420),
        )
        .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))?;
        self.canvas = canvas;
        let drift = Arc::clone(&self.drift);
        self.media = Some(VideoLoop::spawn(
            canvas,
            writer,
            self.reporter.clone(),
            move |frame, pts| {
                let px_per_sec = drift.load(Ordering::Relaxed) as f64 / 1000.0;
                let seconds = pts as f64 / 1_000_000_000.0;
                draw_bars(frame, canvas, (px_per_sec * seconds) as i64);
            },
        ));
        Ok(StartResult { latency_ms: Some(0) })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.media = None;
        Ok(())
    }

    /// The full validated object, not a diff. It lands on the next frame.
    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        self.drift.store(drift_from(&params), Ordering::Relaxed);
        Ok(Configure::applied())
    }

    fn health(&mut self) -> Health {
        match self.media.as_ref() {
            Some(running) if !running.is_running() => Health::failing("the media loop stopped"),
            _ => Health::ok(),
        }
    }
}

fn main() {
    let env = PluginEnv::from_env();
    // Running from a checkout with no manifest beside the binary is normal for
    // this example, so it carries one that matches what it does.
    let manifest = Manifest::load(env.root.join("gmx-plugin.toml"))
        .or_else(|_| Manifest::parse(include_str!("colour-bars.toml")))
        .expect("the built in manifest");
    let plugin = ColourBars {
        canvas: Canvas::default(),
        drift: Arc::new(AtomicI64::new(0)),
        media: None,
        reporter: None,
    };
    if let Err(e) = runtime::run(&manifest, SourceHandler(plugin)) {
        eprintln!("colour bars stopped: {e}");
        std::process::exit(1);
    }
}
