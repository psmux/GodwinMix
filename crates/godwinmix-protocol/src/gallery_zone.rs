//! Where a gallery item sits on the canvas when it is placed.

use serde::{Deserialize, Serialize};

/// Where an item sits on the canvas when it is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Zone {
    /// The whole canvas, under everything else: a background.
    Full,
    /// The lower third, inside title safe.
    LowerThird,
    /// A corner bug: top right, inside title safe.
    Bug,
    /// A strip across the top.
    Top,
    /// A strip across the bottom: a ticker.
    Bottom,
    /// The middle of the canvas: a title card, a quote.
    Center,
    /// Over the whole canvas, on top: a graphic laid out on a full frame
    /// with transparency where there is nothing.
    #[default]
    Overlay,
}

impl Zone {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::LowerThird => "lower-third",
            Self::Bug => "bug",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Center => "center",
            Self::Overlay => "overlay",
        }
    }

    /// Read a zone the common ways: `background` and `fullscreen` are full,
    /// `l3` and `strap` a lower third, `corner` and `logo` a bug, `ticker` the
    /// bottom strip, `title` and `card` the centre.
    pub fn parse(text: &str) -> Option<Self> {
        let t = normal(text);
        Some(match t.as_str() {
            "full" | "background" | "bg" | "backdrop" | "fullscreen" | "full-screen" | "full-frame" | "plate" => Self::Full,
            "lower-third" | "lowerthird" | "l3" | "lower" | "strap" | "name-strap" | "lower-3rd" => Self::LowerThird,
            "bug" | "corner" | "logo" | "corner-bug" | "top-right" | "watermark" => Self::Bug,
            "top" | "header" | "top-strip" | "top-bar" => Self::Top,
            "bottom" | "ticker" | "crawl" | "bottom-strip" | "bottom-bar" => Self::Bottom,
            "center" | "centre" | "middle" | "title" | "title-card" | "card" | "quote" => Self::Center,
            "overlay" | "over" | "canvas" | "whole" | "none" | "auto" => Self::Overlay,
            _ => return None,
        })
    }
}

/// Lower case, with spaces and underscores as dashes.
pub(crate) fn normal(text: &str) -> String {
    text.trim().to_ascii_lowercase().replace([' ', '_'], "-")
}

