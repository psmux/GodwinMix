//! What a ticker is told: its words, its pace and its look.

use super::super::text::{from_address, style};
use crate::config::Params;
use crate::overlay::{Direction, Motion};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use style::Look;

/// Which way the words go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Way {
    /// Right to left, a news crawl.
    #[default]
    Left,
    /// Left to right.
    Right,
    /// Bottom to top, credits.
    Up,
}

/// The params of `ticker/source`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct TickerParams {
    /// The items, shown one after another with `separator` between them.
    /// Rolling up, each item is a line.
    pub items: Vec<String>,
    /// One item, for a ticker of a single message. Ignored when `items` is
    /// given. Left out too, the part of the address after `ticker:` is used.
    pub text: String,
    /// What goes between items, and between the end of the list and its
    /// start again.
    pub separator: String,
    /// Pixels a second.
    pub speed: f64,
    pub direction: Way,
    /// Go round again for ever. Off, the words cross once and the bar
    /// empties.
    #[serde(rename = "loop")]
    pub repeat: bool,
    /// Width of the bar at the item's own size, in pixels. The scene's box
    /// is what it is drawn in; this is only its shape before it is placed.
    pub width: u32,
    #[serde(flatten)]
    pub look: Look,
}

impl Default for TickerParams {
    fn default() -> Self {
        TickerParams {
            items: Vec::new(),
            text: String::new(),
            separator: "   \u{2022}   ".into(),
            speed: 120.0,
            direction: Way::Left,
            repeat: true,
            width: 1920,
            look: Look { padding: 12.0, radius: 0.0, size: 40.0, ..Look::default() },
        }
    }
}

const KEYS: &[&str] = &["items", "text", "separator", "speed", "direction", "loop", "width"];

/// Check and read `ticker/source` params.
pub fn validate(params: &Params) -> Result<TickerParams> {
    style::known_keys("ticker/source", params, &[KEYS, style::LOOK_KEYS].concat())?;
    let mut p: TickerParams = style::read("ticker/source", params)?;
    p.look.check("ticker/source")?;
    anyhow::ensure!(
        p.speed.is_finite() && (0.0..=5000.0).contains(&p.speed),
        "ticker/source params.speed is pixels a second, 0 to 5000; got {}",
        p.speed
    );
    if p.items.is_empty() && p.text.is_empty() {
        p.text = from_address(params, "ticker:");
    }
    Ok(p)
}

pub fn schema() -> Value {
    serde_json::to_value(schemars::schema_for!(TickerParams)).unwrap_or(Value::Null)
}

impl TickerParams {
    pub fn words(&self) -> Vec<String> {
        if self.items.is_empty() { vec![self.text.clone()] } else { self.items.clone() }
    }

    /// The whole strip as one text: a line of items, or a column for credits.
    pub fn strip(&self) -> String {
        let words = self.words();
        match self.direction {
            Way::Up => words.join("\n"),
            _ if self.repeat => format!("{}{}", words.join(&self.separator), self.separator),
            _ => words.join(&self.separator),
        }
    }

    pub fn motion(&self) -> Motion {
        let direction = match self.direction {
            Way::Left => Direction::Left,
            Way::Right => Direction::Right,
            Way::Up => Direction::Up,
        };
        let gap = if self.direction == Way::Up { (self.look.size * 2.0) as u32 } else { 0 };
        Motion::Crawl { speed: self.speed, direction, gap, repeat: self.repeat }
    }
}

