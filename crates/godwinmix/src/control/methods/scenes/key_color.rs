//! `source.key_color`: the colour to key a camera on, read off a still of it.
//!
//! With a point, it is the colour at that point, which is what a click on the
//! picture in the UI asks for. Without one, it is the screen: the biggest
//! saturated area of one hue in the picture. Both come from the source's tile
//! on the mosaic, the same still `snapshot.get` cuts out, so asking costs one
//! JPEG decode and nothing runs between asks.

use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::plugin::filters::chroma::still::{self, RgbImage};
use godwinmix_core::plugin::filters::chroma::{guess, params::Family, to_hex};
use godwinmix_core::snapshot::{self, Pick};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "source.key_color",
            Scope::Read,
            "The colour to key a source on: the colour at a point of its picture, or with \
             no point the green or blue screen it stands in front of.",
            handler(key_color),
        )
        .params(schema_of::<KeyColorRequest>)
        .result(schema_of::<KeyColor>)
        .tool(
            "key_color",
            Tier::Search,
            "Find the colour to key a camera on. Give the source's `id`, and either `x` and `y` (0 to \
             1 across and down its picture) for the colour at that point, or neither for the \
             screen colour the picture is mostly made of. Put the answer's `color` into the \
             presenter's chroma/filter as `color`.",
        ),
    );
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyColorRequest {
    /// The source id.
    pub id: String,
    /// 0 to 1 across the source's picture. With `y`, the colour there.
    #[serde(default)]
    pub x: Option<f64>,
    /// 0 to 1 down the source's picture.
    #[serde(default)]
    pub y: Option<f64>,
    /// "green" or "blue" to look for that screen only. Either when left out.
    #[serde(default)]
    pub screen: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KeyColor {
    /// "#rrggbb".
    pub color: String,
    /// "point" for the colour at a point, "green" or "blue" for a screen.
    pub found: String,
    /// For a screen, the share of the picture it covers, 0 to 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<f32>,
}

async fn key_color(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: KeyColorRequest = call.params(&params)?;
    let family = match req.screen.as_deref() {
        None | Some("") => Family::Any,
        Some("green") => Family::Green,
        Some("blue") => Family::Blue,
        Some(other) => {
            let e = format!("screen is {other:?}; give \"green\" or \"blue\", or leave it out.");
            return Err(RpcError::invalid_params(e).with("field", "screen"));
        }
    };
    let still = still_of(&call, &req.id).await?;
    let answer = match (req.x, req.y) {
        (Some(x), Some(y)) if (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) => {
            Some(KeyColor { color: to_hex(still::at(&still, x, y)), found: "point".into(), share: None })
        }
        (None, None) => screen(&still, family),
        _ => {
            return Err(RpcError::invalid_params("give both x and y from 0 to 1, or neither for the screen colour.")
                .with("field", "x"))
        }
    };
    match answer {
        Some(a) => body(a),
        None => Err(RpcError::not_in_state(format!(
            "no green or blue screen fills enough of {}'s picture to key on. Click the screen in \
             the picture (x and y), or give the colour yourself.",
            req.id
        ))
        .with("source", req.id)
        .with("min_share", guess::MIN_SHARE)),
    }
}

/// The screen colour of `source`, for a caller that can do without one.
pub async fn guess(call: &Call, source: &str) -> Option<KeyColor> {
    let still = still_of(call, source).await.ok()?;
    screen(&still, Family::Any)
}

fn screen(still: &RgbImage, family: Family) -> Option<KeyColor> {
    let g = still::screen(still, family)?;
    let found = if g.family == Family::Blue { "blue" } else { "green" };
    Some(KeyColor { color: to_hex(g.rgb), found: found.into(), share: Some(g.share) })
}

/// The source's picture as it is now, from its tile on the mosaic.
///
/// A mosaic started by this very ask shows its tiles black for its first
/// frames, so a black still is asked for again, a few times, before it is
/// believed.
async fn still_of(call: &Call, source: &str) -> Result<RgbImage, RpcError> {
    let known = call.source_ids().await?;
    if !known.iter().any(|k| k == source) {
        return Err(RpcError::not_found("source", source, &known));
    }
    if let Some(why) = call.snapshots.disabled_reason() {
        return Err(RpcError::not_in_state(why.message).with_action(why.action).with("source", source));
    }
    let mut still = tile(call, source).await?;
    for _ in 0..4 {
        if !still::is_dark(&still) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        still = tile(call, source).await?;
    }
    Ok(still)
}

async fn tile(call: &Call, source: &str) -> Result<RgbImage, RpcError> {
    let Some(latest) = call.snapshots.latest_wanted(Duration::from_secs(3)).await else {
        return Err(RpcError::not_in_state("no still yet: the mosaic is being built for you. Ask again in a second.")
            .with("retry_after_ms", 1000));
    };
    let cell = snapshot::find_cell(&latest.cells, &Pick::Source(source.to_string())).cloned().ok_or_else(|| {
        RpcError::not_in_state(format!("{source} has no tile on the mosaic yet. Ask again in a second."))
            .with("source", source)
            .with("retry_after_ms", 1000)
    })?;
    let jpeg = latest.jpeg.clone();
    tokio::task::spawn_blocking(move || snapshot::decode_jpeg(&jpeg).map(|m| snapshot::crop_cell(&m, &cell)))
        .await
        .map_err(|e| RpcError::internal(format!("reading the still failed: {e}")))?
        .map_err(|e| RpcError::internal(format!("the mosaic frame could not be decoded: {e}")))
}
