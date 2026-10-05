//! `scene.create_from`: a scene from a set of sources and a layout, in one call.
//!
//! Every way of making a laid out scene goes through here, the green screen
//! presenter included. That used to be a method of its own, `scene.virtual_set`,
//! and a button of its own beside New scene, when all it did was apply the
//! `virtual-set` layout. What it did that a layout cannot do on its own is now
//! done here for any layout:
//!
//! ```text
//!   scene.create_from {sources, layout?, settings?}
//!        |
//!        +-- set_inputs::source_for   a source id is used as it is; a media
//!        |                            file or a path becomes a source, once
//!        +-- set_inputs::key_for      a layout whose `key` setting names a
//!        |                            slot (`x-gmx-key-of`) gets the colour
//!        |                            guessed from that camera
//!        \-- ops::create_from_with    the layout, applied as a new scene
//! ```

use super::{client, scene_error, server, set_inputs, CreateFromAnswer, CreateFromRequest};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::scene::layout;
use godwinmix_core::scene::server::ops;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.create_from",
            Scope::Operate,
            "A scene from a set of sources, laid out by the built in layout for that count \
             (full, two-box, three-box, quad, then a grid) or by a named one. Pictures \
             from the media library become sources, and a keyed layout such as \
             virtual-set guesses its key colour from the camera.",
            handler(create_from),
        )
        .params(schema_of::<CreateFromRequest>)
        .result(schema_of::<CreateFromAnswer>)
        .not_idempotent()
        .tool(
            "create_scene_from",
            Tier::Standard,
            "Make a scene out of a list of sources in one call. With no `layout` the count \
             picks one: one source fills the canvas, two make a two box, three a three \
             box, four a quad, more a grid. A source may also be a media library file \
             name from `list_media` or a path, which becomes a source. To put a presenter \
             in front of a new background, use layout \"virtual-set\" with sources \
             [background, camera, optional foreground such as a desk]. With a green screen \
             behind them the key colour is guessed from the camera unless `settings.key` is \
             \"#rrggbb\"; `settings.screen` \"blue\" is for a blue screen, and \"none\" cuts \
             the person out with a model when there is no screen at all. \
             `settings.presenter_scale` (0.3 to 1) and `settings.presenter_x` (0 to 1) \
             place the presenter. The items are named after their sources, or after their \
             role in the layout. Returns the scene with every item's box in pixels.",
        ),
    );
}

async fn create_from(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: CreateFromRequest = call.params(&params)?;
    if req.sources.is_empty() {
        return Err(RpcError::invalid_params(
            "scene.create_from needs at least one source. Read the ids from source.list.",
        ));
    }
    set_inputs::check_ranges(
        req.settings.get("presenter_scale").and_then(Value::as_f64),
        req.settings.get("presenter_x").and_then(Value::as_f64),
    )?;
    // A name the mixer does not know would be a scene that draws nothing, so
    // it becomes a source when it is a file and is refused when it is not.
    let known = call.source_ids().await?;
    let mut added = Vec::new();
    let mut sources = Vec::with_capacity(req.sources.len());
    for (i, named) in req.sources.iter().enumerate() {
        let field = format!("sources[{i}]");
        sources.push(set_inputs::source_for(&call, &known, named, &field, &mut added).await?);
    }
    let mut settings: layout::Values = req.settings.clone().into_iter().collect();
    let screen = screen_of(&mut settings, req.layout.as_deref(), &sources)?;
    if screen == Screen::None {
        cutout_ready()?;
    }
    let (key, key_from) = match keyed_slot(req.layout.as_deref(), &sources).filter(|_| screen != Screen::None) {
        Some(camera) => {
            let asked = settings.get("key").and_then(Value::as_str).map(str::to_string);
            let (key, from) = set_inputs::key_for(&call, &camera, asked.as_deref()).await?;
            settings.insert("key".into(), json!(key));
            (Some(key), Some(from.to_string()))
        }
        None => (None, None),
    };
    let (id, _) = server(&call)
        .edit(client(&call).as_deref(), |doc| {
            let mut scene = ops::create_from_with(doc, &sources, req.layout.as_deref(), req.name.as_deref(), &settings)?;
            screen.apply(&mut scene.items);
            let id = scene.id;
            doc.scenes.push(scene);
            Ok(id)
        })
        .map_err(|e| scene_error(&call, e))?;
    let scene = server(&call).scene(&id.to_string()).map_err(|e| scene_error(&call, e))?;
    body(CreateFromAnswer { scene, added, key, key_from })
}

/// The source in the slot a layout's `key` setting keys, when it has one.
fn keyed_slot(layout_name: Option<&str>, sources: &[String]) -> Option<String> {
    let preset = layout::builtin(layout_name?).ok()?;
    let slot = preset.params.pointer("/properties/key/x-gmx-key-of")?.as_str()?.to_string();
    let index = ops::slot_names(&preset).iter().position(|s| *s == slot)?;
    sources.get(index).cloned()
}

/// What is behind the presenter in a keyed layout: a green screen, a blue
/// one, or no screen at all, when the person is cut out by a model instead
/// (`matte/filter`). The layout itself always names the key; this swaps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Green,
    Blue,
    None,
}

/// The `screen` setting, taken out of the layout's own settings.
fn screen_of(settings: &mut layout::Values, layout_name: Option<&str>, sources: &[String]) -> Result<Screen, RpcError> {
    let Some(asked) = settings.remove("screen") else { return Ok(Screen::Green) };
    if keyed_slot(layout_name, sources).is_none() {
        return Err(RpcError::invalid_params(
            "settings.screen is for a layout that keys a presenter, such as virtual-set; this              layout keys nothing. Leave it out.",
        )
        .with("field", "settings.screen"));
    }
    match asked.as_str().unwrap_or_default() {
        "green" | "" => Ok(Screen::Green),
        "blue" => Ok(Screen::Blue),
        "none" => Ok(Screen::None),
        other => Err(RpcError::invalid_params(format!(
            "settings.screen is {other:?}; it is green, blue, or none for a presenter with no              screen behind them, who is then cut out by a model."
        ))
        .with("field", "settings.screen")),
    }
}

impl Screen {
    /// Put the right filter on every keyed item.
    fn apply(self, items: &mut [godwinmix_core::scene::Item]) {
        for item in items {
            for f in item.filters.iter_mut().filter(|f| f.kind == "chroma/filter") {
                match self {
                    Screen::Green => {}
                    Screen::Blue => {
                        if let Some(p) = f.params.as_object_mut() {
                            p.insert("method".into(), json!("blue"));
                        }
                    }
                    Screen::None => {
                        f.kind = "matte/filter".into();
                        f.name = Some("Cutout".into());
                        f.params = json!({});
                    }
                }
            }
        }
    }
}

/// Whether this machine can cut a person out, asked before the scene is made.
#[cfg(feature = "matte")]
fn cutout_ready() -> Result<(), RpcError> {
    godwinmix_core::plugin::filters::matte::ready(&Default::default())
        .map(|_| ())
        .map_err(|e| RpcError::not_in_state(format!("{e:#}. Nothing was changed.")).with("field", "settings.screen"))
}

#[cfg(not(feature = "matte"))]
fn cutout_ready() -> Result<(), RpcError> {
    Err(RpcError::not_in_state(
        "this build has no person cutout (it was built without the matte feature), so a \
         presenter needs a green or blue screen. Leave settings.screen out.",
    )
    .with("field", "settings.screen"))
}
