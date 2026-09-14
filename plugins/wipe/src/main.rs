//! A wipe, as a transition plugin.
//!
//! The worked example of the fourth plugin kind. It carries no media, opens no
//! socket and touches no pipeline: the core gives it the pads on the way out
//! and the pads on the way in, asks what each one should look like at a
//! fraction of the way through, and binds the answers as control sources the
//! compositor reads on the frame. A transition written here is therefore
//! exactly as accurate as one compiled into the core.
//!
//! ```text
//!   core                          wipe
//!    |  render {from, to, 0.0}     |
//!    | --------------------------> |
//!    | <-------------------------- |  {pads: {sink_2: {xpos: 1920}, ...}}
//!    |  render {from, to, 0.033}   |
//!    | --------------------------> |    ... once per frame of the window,
//!    |                             |    before the window starts
//! ```
//!
//! # What a wipe is
//!
//! The incoming scene enters from one edge and travels across the canvas. With
//! `push` on, which is the default, the outgoing scene is pushed out of the
//! opposite edge at the same rate, so the two move as one strip; with it off
//! the incoming scene slides over a scene that stays where it is.
//!
//! Both scenes are on the canvas for the length of the transition, which the
//! core arranges by binding the incoming scene to slots of its own. That is
//! why the pad ids differ between `from` and `to` even when the same camera is
//! in both.
//!
//! # Why the geometry is the canvas and not the pad
//!
//! `render` is told the pad ids, not where each pad sits. A wipe moves the
//! whole picture, so the distance to travel is the canvas, which arrives in
//! the handshake. A transition that needs to know where each item is would ask
//! for the geometry; this one does not, and is smaller for it.

use godwinmix_sdk::prelude::*;
use serde_json::{json, Map, Value};

/// Which way the incoming scene travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Direction {
    /// In from the right edge, travelling left. What a viewer reads as a left
    /// wipe, and the default because it matches the reading direction of the
    /// scripts most of this project's users write in.
    #[default]
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    fn parse(name: &str) -> Direction {
        match name.trim().to_lowercase().as_str() {
            "right" => Direction::Right,
            "up" => Direction::Up,
            "down" => Direction::Down,
            _ => Direction::Left,
        }
    }

    /// The property this direction moves, and which way the numbers run.
    fn axis(self) -> (&'static str, f64) {
        match self {
            Direction::Left => ("xpos", 1.0),
            Direction::Right => ("xpos", -1.0),
            Direction::Up => ("ypos", 1.0),
            Direction::Down => ("ypos", -1.0),
        }
    }

    /// How far a picture has to travel to be off the canvas.
    fn distance(self, canvas: Canvas) -> f64 {
        match self {
            Direction::Left | Direction::Right => canvas.width as f64,
            Direction::Up | Direction::Down => canvas.height as f64,
        }
    }
}

/// The settings, as `schemas/wipe.json` describes them.
#[derive(Debug, Clone, Copy)]
struct Settings {
    direction: Direction,
    push: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { direction: Direction::default(), push: true }
    }
}

impl Settings {
    fn from_params(params: &Value) -> Settings {
        Settings {
            direction: params
                .get("direction")
                .and_then(Value::as_str)
                .map(Direction::parse)
                .unwrap_or_default(),
            push: params.get("push").and_then(Value::as_bool).unwrap_or(true),
        }
    }
}

struct Wipe {
    canvas: Canvas,
    settings: Settings,
}

impl Transition for Wipe {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        self.canvas = ready.canvas;
        self.settings = Settings::from_params(&ready.params);
        reporter.info(format!(
            "wipe is up: {:?}, push {}, canvas {}x{}",
            self.settings.direction, self.settings.push, self.canvas.width, self.canvas.height
        ));
        Ok(InitializeResult::default())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        // Every setting takes effect on the next take, so nothing here needs a
        // restart. A transition is asked for its shape once per transition and
        // holds no state between them.
        self.settings = Settings::from_params(&params);
        Ok(Configure::applied())
    }

    fn render(&mut self, render: &Render) -> Result<Value, RpcError> {
        Ok(json!({ "pads": self.pads(render) }))
    }
}

impl Wipe {
    /// Where every pad sits at this point of the wipe.
    fn pads(&self, render: &Render) -> Map<String, Value> {
        let (axis, sign) = self.settings.direction.axis();
        let distance = self.settings.direction.distance(self.canvas);
        let t = render.progress.clamp(0.0, 1.0);
        let mut pads = Map::new();
        // The incoming scene starts one canvas away, on the far side of the
        // direction of travel, and arrives at zero.
        let arriving = sign * distance * (t - 1.0);
        for pad in &render.to {
            pads.insert(
                pad.clone(),
                json!({ axis: arriving.round(), "alpha": 1.0 }),
            );
        }
        // The outgoing scene is pushed out ahead of it, or left where it is.
        for pad in &render.from {
            let leaving = match self.settings.push {
                true => sign * distance * t,
                false => 0.0,
            };
            pads.insert(
                pad.clone(),
                json!({ axis: leaving.round(), "alpha": 1.0 }),
            );
        }
        pads
    }
}

fn main() {
    let env = PluginEnv::from_env();
    let manifest = Manifest::load(env.root.join("gmx-plugin.toml")).expect("gmx-plugin.toml");
    let wipe = Wipe { canvas: Canvas::default(), settings: Settings::default() };
    runtime::run(&manifest, TransitionHandler(wipe)).expect("the plugin stopped badly");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wipe(direction: Direction, push: bool) -> Wipe {
        Wipe {
            canvas: Canvas { width: 1920, height: 1080, fps: 30 },
            settings: Settings { direction, push },
        }
    }

    fn render(progress: f64) -> Render {
        Render {
            from: vec!["sink_0".into()],
            to: vec!["sink_1".into()],
            progress,
            running_time_ns: 0,
        }
    }

    /// The three points the harness checks, spelled out: the incoming scene is
    /// one canvas away at the start, half a canvas away in the middle, and
    /// home at the end.
    #[test]
    fn the_incoming_scene_travels_one_canvas_and_lands_on_zero() {
        let wipe = wipe(Direction::Left, true);
        let at = |t: f64| wipe.pads(&render(t))["sink_1"]["xpos"].as_f64().expect("an xpos");
        assert_eq!(at(0.0), -1920.0, "it starts off the right edge");
        assert_eq!(at(0.5), -960.0, "half way there");
        assert_eq!(at(1.0), 0.0, "and lands exactly on the canvas");
    }

    #[test]
    fn push_moves_the_outgoing_scene_and_no_push_leaves_it() {
        let pushed = wipe(Direction::Left, true);
        let over = wipe(Direction::Left, false);
        let out = |w: &Wipe, t: f64| w.pads(&render(t))["sink_0"]["xpos"].as_f64().expect("an xpos");
        assert_eq!(out(&pushed, 0.0), 0.0);
        assert_eq!(out(&pushed, 1.0), 1920.0, "pushed out of the far edge");
        assert_eq!(out(&over, 1.0), 0.0, "left where it was");
    }

    #[test]
    fn every_direction_moves_the_property_it_should_and_the_right_way() {
        for (direction, axis, at_start) in [
            (Direction::Left, "xpos", -1920.0),
            (Direction::Right, "xpos", 1920.0),
            (Direction::Up, "ypos", -1080.0),
            (Direction::Down, "ypos", 1080.0),
        ] {
            let w = wipe(direction, true);
            let start = w.pads(&render(0.0));
            let end = w.pads(&render(1.0));
            assert_eq!(
                start["sink_1"][axis].as_f64(),
                Some(at_start),
                "{direction:?} should start one canvas away on {axis}"
            );
            assert_eq!(end["sink_1"][axis].as_f64(), Some(0.0), "{direction:?} must land at 0");
        }
    }

    #[test]
    fn a_direction_nobody_wrote_is_the_default_rather_than_an_error() {
        assert_eq!(Direction::parse("sideways"), Direction::Left);
        assert_eq!(Direction::parse("  UP "), Direction::Up);
        let settings = Settings::from_params(&json!({"direction": "down", "push": false}));
        assert_eq!(settings.direction, Direction::Down);
        assert!(!settings.push);
        // And nothing at all is the documented default.
        let settings = Settings::from_params(&json!({}));
        assert_eq!(settings.direction, Direction::Left);
        assert!(settings.push);
    }

    /// Every pad the core offers comes back, and nothing else. The harness
    /// refuses a plugin that invents a pad name, so this is the same rule
    /// checked from the plugin's side.
    #[test]
    fn every_pad_the_core_named_is_answered_and_no_others() {
        let wipe = wipe(Direction::Left, true);
        let request = Render {
            from: vec!["sink_0".into(), "sink_1".into()],
            to: vec!["sink_2".into(), "sink_3".into()],
            progress: 0.5,
            running_time_ns: 0,
        };
        let pads = wipe.pads(&request);
        assert_eq!(pads.len(), 4);
        for pad in ["sink_0", "sink_1", "sink_2", "sink_3"] {
            assert!(pads.contains_key(pad), "{pad} was not answered");
        }
    }
}
