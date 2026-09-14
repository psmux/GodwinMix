//! A complete source plugin: eight colour bars at canvas caps.
//!
//! This is the worked plugin from the plugin architecture appendix, written in
//! Rust against this crate. It is the thing `gmx plugin new --lang rust` will
//! produce. Change `draw_bars` and the settings schema and you have a different
//! source; nothing else here needs touching.
//!
//! Run it by hand to see what the core sees:
//!
//! ```sh
//! echo '{"jsonrpc":"2.0","id":0,"result":{"core":"godwinmix","version":"0","api_level":1,
//!   "api_compatible":1,"canvas":{"width":320,"height":180,"fps":30},"transport":"container",
//!   "media":"","instance":"bars","provide":"source","params":{}}}' \
//!   | cargo run --example colour-bars > bars.mkv
//! ```

use godwinmix_sdk::prelude::*;
use serde_json::Value;

/// The eight bars, as Y, U and V. These are the standard bar values; the
/// chroma pair is constant down a bar, so the U and V planes are cheap.
const BARS: [[u8; 3]; 8] = [
    [235, 128, 128], // white
    [210, 16, 146],  // yellow
    [170, 166, 16],  // cyan
    [145, 54, 34],   // green
    [106, 202, 222], // magenta
    [81, 90, 240],   // red
    [41, 240, 110],  // blue
    [16, 128, 128],  // black
];

struct ColourBars {
    canvas: Canvas,
    /// Moves the bars sideways by this many pixels per second. 0 holds still.
    drift: f64,
    media: Option<VideoLoop>,
    reporter: Option<Reporter>,
}

impl ColourBars {
    fn new() -> ColourBars {
        ColourBars {
            canvas: Canvas::default(),
            drift: 0.0,
            media: None,
            reporter: None,
        }
    }

    fn drift_from(params: &Value) -> f64 {
        params
            .get("drift_px_per_sec")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    }
}

/// Fill one I420 frame with the bars, offset by `shift` pixels.
fn draw_bars(frame: &mut [u8], canvas: Canvas, shift: i64) {
    let width = canvas.width as usize;
    let height = canvas.height as usize;
    let cw = width.div_ceil(2);
    let ch = height.div_ceil(2);
    let (y_plane, chroma) = frame.split_at_mut(width * height);
    let (u_plane, v_plane) = chroma.split_at_mut(cw * ch);

    // One row of each plane is computed, then copied down. The bars do not
    // change with the row, so drawing 1080 of them would be 1080 times the work
    // for the same picture.
    let bar_of = |x: usize| -> usize {
        let bar_width = (width as i64 / 8).max(1);
        let shifted = (x as i64 - shift).rem_euclid(width.max(1) as i64);
        ((shifted / bar_width) as usize).min(7)
    };
    for x in 0..width {
        y_plane[x] = BARS[bar_of(x)][0];
    }
    for x in 0..cw {
        let bar = BARS[bar_of(x * 2)];
        u_plane[x] = bar[1];
        v_plane[x] = bar[2];
    }
    for row in 1..height {
        y_plane.copy_within(0..width, row * width);
    }
    for row in 1..ch {
        u_plane.copy_within(0..cw, row * cw);
        v_plane.copy_within(0..cw, row * cw);
    }
}

impl Source for ColourBars {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.canvas = ready.canvas;
        self.drift = Self::drift_from(&ready.params);
        reporter.info(format!(
            "colour bars at {}x{}@{} for instance '{}'",
            ready.canvas.width, ready.canvas.height, ready.canvas.fps, ready.instance
        ));
        self.reporter = Some(reporter);
        // Bars are drawn on demand, so nothing is buffered and nothing is late.
        Ok(InitializeResult {
            latency_ms: Some(0),
        })
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        let canvas = params.canvas;
        self.canvas = canvas;
        let writer = media::open(
            params.transport,
            &params.media,
            canvas,
            media::Streams::video_only(media::VideoFormat::I420),
        )
        .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))?;
        let drift = self.drift;
        self.media = Some(VideoLoop::spawn(
            canvas,
            writer,
            self.reporter.clone(),
            move |frame, pts| {
                let seconds = pts as f64 / 1_000_000_000.0;
                draw_bars(frame, canvas, (drift * seconds) as i64);
            },
        ));
        Ok(StartResult {
            latency_ms: Some(0),
        })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.media = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        // Drift is read once when the loop starts, so changing it takes a
        // restart of the media loop, not of the process.
        self.drift = Self::drift_from(&params);
        if self.media.is_some() {
            let start = StartParams {
                canvas: self.canvas,
                transport: Transport::Container,
                media: String::new(),
            };
            self.stop()?;
            self.start(&start)?;
        }
        Ok(Configure::applied())
    }

    fn health(&mut self) -> Health {
        match self.media.as_ref() {
            Some(loop_) if !loop_.is_running() => Health::failing("the media loop stopped"),
            _ => Health::ok(),
        }
    }
}

fn main() {
    let env = PluginEnv::from_env();
    let manifest_path = env.root.join("gmx-plugin.toml");
    let manifest = match Manifest::load(&manifest_path) {
        Ok(m) => m,
        // Running from a checkout with no manifest beside the binary is normal
        // for this example, so fall back to one that matches what it does.
        Err(_) => Manifest::parse(include_str!("colour-bars.toml")).expect("the built in manifest"),
    };
    if let Err(e) = runtime::run(&manifest, SourceHandler(ColourBars::new())) {
        eprintln!("colour bars stopped: {e}");
        std::process::exit(1);
    }
}
