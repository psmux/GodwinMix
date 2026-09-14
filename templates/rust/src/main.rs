//! {{description}}
//!
//! A GodwinMix source plugin. It draws colour bars at the canvas caps and
//! writes them over the transport the core chose. Control is JSON-RPC 2.0, one
//! object per line, on stdin and stderr; the SDK runs that loop.
//!
//! Change `draw` and `schemas/source.json`. Nothing else here needs touching.

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

/// What the settings schema lets an operator change.
#[derive(Clone, Copy)]
struct Settings {
    bars: usize,
}

impl Settings {
    fn from(params: &Value) -> Settings {
        let bars = params
            .get("bars")
            .and_then(Value::as_u64)
            .unwrap_or(8)
            .clamp(1, 8) as usize;
        Settings { bars }
    }
}

// --- what you change --------------------------------------------------------

/// Fill one I420 frame. This is the one function your plugin rewrites.
///
/// `frame` is already exactly the right size: a Y plane of `width * height`
/// bytes, then a U plane and a V plane of `ceil(width/2) * ceil(height/2)`
/// each. Y is brightness, 16 is black and 235 is white; U and V are colour,
/// 128 each is grey.
///
/// This runs on the media thread. Keep it to drawing. A network call in here
/// is a dropped frame.
fn draw(frame: &mut [u8], canvas: Canvas, settings: Settings, _pts_ns: u64) {
    let width = canvas.width as usize;
    let height = canvas.height as usize;
    let cw = width.div_ceil(2);
    let ch = height.div_ceil(2);
    let (luma, chroma) = frame.split_at_mut(width * height);
    let (u, v) = chroma.split_at_mut(cw * ch);

    let bar_at = |x: usize| BARS[(x * settings.bars / width).min(settings.bars - 1)];

    // One row of each plane, then copied down. The bars do not change with the
    // row, so drawing 1,080 of them would be 1,080 times the work.
    for x in 0..width {
        luma[x] = bar_at(x)[0];
    }
    for x in 0..cw {
        u[x] = bar_at(x * 2)[1];
        v[x] = bar_at(x * 2)[2];
    }
    for row in 1..height {
        luma.copy_within(0..width, row * width);
    }
    for row in 1..ch {
        u.copy_within(0..cw, row * cw);
        v.copy_within(0..cw, row * cw);
    }
}

// --- the plugin -------------------------------------------------------------

struct Plugin {
    canvas: Canvas,
    settings: Settings,
    reporter: Option<Reporter>,
    media: Option<VideoLoop>,
}

impl Plugin {
    fn new() -> Plugin {
        Plugin {
            canvas: Canvas::default(),
            settings: Settings { bars: 8 },
            reporter: None,
            media: None,
        }
    }

    /// Open the transport and start the loop. Used by `start` and by the
    /// restart that a `configure` needs while running.
    fn open(&mut self, params: &StartParams) -> Result<(), RpcError> {
        let canvas = params.canvas;
        let settings = self.settings;
        let writer = media::open(
            params.transport,
            &params.media,
            canvas,
            media::Streams::video_only(media::VideoFormat::I420),
        )
        .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))?;
        self.canvas = canvas;
        self.media = Some(VideoLoop::spawn(
            canvas,
            writer,
            self.reporter.clone(),
            move |frame, pts| draw(frame, canvas, settings, pts),
        ));
        Ok(())
    }
}

impl Source for Plugin {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.canvas = ready.canvas;
        self.settings = Settings::from(&ready.params);
        reporter.info(format!(
            "{{name}} at {}x{}@{} as '{}'",
            ready.canvas.width, ready.canvas.height, ready.canvas.fps, ready.instance
        ));
        self.reporter = Some(reporter);
        // Frames are drawn on demand, so nothing is buffered and nothing is late.
        Ok(InitializeResult {
            latency_ms: Some(0),
        })
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        self.open(params)?;
        Ok(StartResult {
            latency_ms: Some(0),
        })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        // Dropping the loop stops its thread and closes the writer.
        self.media = None;
        Ok(())
    }

    /// The full validated object, not a diff.
    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        self.settings = Settings::from(&params);
        if self.media.is_some() {
            // The loop captured the old settings, so it is opened again. The
            // core covers the gap with a freeze frame.
            let restart = StartParams {
                canvas: self.canvas,
                transport: Transport::Container,
                media: String::new(),
            };
            self.stop()?;
            self.open(&restart)?;
        }
        Ok(Configure::applied())
    }

    fn health(&mut self) -> Health {
        match self.media.as_ref() {
            Some(running) if !running.is_running() => {
                Health::failing("the media loop stopped. The supervisor will restart this process.")
            }
            Some(running) => Health {
                state: HealthState::Ok,
                detail: Some(format!("{} frames sent", running.frames())),
                latency_ms: Some(0),
            },
            None => Health::ok(),
        }
    }
}

fn main() {
    let env = PluginEnv::from_env();
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = runtime::run(&manifest, SourceHandler(Plugin::new())) {
        eprintln!("{{name}} stopped: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame `draw` fills must be exactly the size the canvas asks for.
    #[test]
    fn a_frame_is_the_size_the_canvas_asks_for() {
        let canvas = Canvas::new(160, 90, 30);
        let mut frame = vec![0u8; canvas.i420_frame_bytes()];
        draw(&mut frame, canvas, Settings { bars: 8 }, 0);
        assert_eq!(frame.len(), 160 * 90 * 3 / 2);
    }

    #[test]
    fn the_bars_run_white_to_black_across_the_picture() {
        let canvas = Canvas::new(160, 90, 30);
        let mut frame = vec![0u8; canvas.i420_frame_bytes()];
        draw(&mut frame, canvas, Settings { bars: 8 }, 0);
        assert_eq!(frame[0], 235, "the first bar is white");
        assert_eq!(frame[159], 16, "the last bar is black");
        // Every row is the same, so row 50 starts white too.
        assert_eq!(frame[50 * 160], 235);
    }

    #[test]
    fn settings_are_clamped_to_what_the_schema_allows() {
        assert_eq!(Settings::from(&serde_json::json!({"bars": 0})).bars, 1);
        assert_eq!(Settings::from(&serde_json::json!({"bars": 99})).bars, 8);
        assert_eq!(Settings::from(&serde_json::json!({})).bars, 8);
    }
}
