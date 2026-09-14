//! Walking an OBS collection and building the GodwinMix document from it.
//!
//! The mapping table lives beside this in `obs_import.rs`; this is the walk.
//! Every decision it takes that loses something goes into the report, so the
//! operator sees the whole of it in one screen.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use serde_json::{json, Value};

use super::super::geometry::{compose, normalise_crop};
use super::super::id::Id;
use super::*;

/// OBS filter types this build knows a GodwinMix equivalent for. Anything else
/// is carried across under its OBS name so no settings are lost, and named in
/// the report so the operator knows to replace it.
const FILTER_TYPES: &[(&str, &str)] = &[
    ("chroma_key_filter", "chroma/filter"),
    ("chroma_key_filter_v2", "chroma/filter"),
    ("color_key_filter", "colorkey/filter"),
    ("color_key_filter_v2", "colorkey/filter"),
    ("color_filter", "color/filter"),
    ("color_filter_v2", "color/filter"),
    ("crop_filter", "crop/filter"),
    ("sharpness_filter", "sharpen/filter"),
    ("sharpness_filter_v2", "sharpen/filter"),
    ("mask_filter", "mask/filter"),
    ("mask_filter_v2", "mask/filter"),
    ("gain_filter", "gain/filter"),
    ("noise_gate_filter", "gate/filter"),
    ("noise_suppress_filter", "denoise/filter"),
    ("noise_suppress_filter_v2", "denoise/filter"),
    ("compressor_filter", "compressor/filter"),
    ("limiter_filter", "limiter/filter"),
    ("expander_filter", "expander/filter"),
    ("async_delay_filter", "delay/filter"),
];

/// The state of one import.
pub(super) struct Importer<'a> {
    raw: ObsCollection,
    options: &'a Options,
    canvas: Canvas,
    /// Every source by uuid and by name, because OBS files in the wild use
    /// either and the newer ones write both.
    by_uuid: BTreeMap<String, usize>,
    by_name: BTreeMap<String, usize>,
    /// What each source mapped to, by its index in `raw.sources`.
    mapped: Vec<Mapped>,
    /// The GodwinMix source id for each source that became one.
    ids: BTreeMap<usize, String>,
    /// The scene document id for each OBS scene, by source index.
    scene_ids: BTreeMap<usize, Id>,
    /// How many items use each source.
    placements: BTreeMap<usize, usize>,
    filters: Vec<FilterReport>,
    notes: Vec<String>,
    /// Groups being expanded, so a group that contains itself cannot loop.
    open_groups: Vec<usize>,
}

impl<'a> Importer<'a> {
    pub(super) fn new(mut raw: ObsCollection, options: &'a Options) -> Importer<'a> {
        // Older collections keep groups in their own array. Treat them as what
        // they are: sources with items.
        let groups = std::mem::take(&mut raw.groups);
        for group in groups {
            if !raw.sources.iter().any(|s| s.name == group.name) {
                raw.sources.push(group);
            }
        }
        let mut by_uuid = BTreeMap::new();
        let mut by_name = BTreeMap::new();
        for (index, source) in raw.sources.iter().enumerate() {
            if let Some(uuid) = &source.uuid {
                by_uuid.insert(uuid.clone(), index);
            }
            by_name.entry(source.name.clone()).or_insert(index);
        }
        Importer {
            canvas: options.canvas.unwrap_or_default(),
            options,
            by_uuid,
            by_name,
            mapped: Vec::new(),
            ids: BTreeMap::new(),
            scene_ids: BTreeMap::new(),
            placements: BTreeMap::new(),
            filters: Vec::new(),
            notes: Vec::new(),
            open_groups: Vec::new(),
            raw,
        }
    }

    /// Do the import.
    pub(super) fn run(mut self) -> Result<Import> {
        self.classify();
        self.mint_ids();
        let scenes = self.build_scenes();
        let name = self.raw.name.clone().unwrap_or_else(|| "Imported from OBS".into());
        let mut document = Collection::new(name.clone(), self.canvas);
        document.scenes = scenes;
        let items = document.scenes.iter().map(|s| s.walk().len()).sum();
        if self.options.canvas.is_none() {
            self.notes.push(
                "OBS keeps the canvas size in its profile, not in the collection, so this import assumed 1920x1080. Pass --canvas WIDTHxHEIGHT if yours is different."
                    .into(),
            );
        }
        let report = Report {
            collection: name,
            canvas: self.canvas,
            scenes: document.scenes.len(),
            items,
            sources: self.source_reports(),
            filters_duplicated: std::mem::take(&mut self.filters),
            notes: std::mem::take(&mut self.notes),
        };
        document.check_refs()?;
        Ok(Import { document, sources: self.imported_sources(), report })
    }

    /// Run the mapping table over every source once.
    fn classify(&mut self) {
        self.mapped = self
            .raw
            .sources
            .iter()
            .map(|source| match source.id.as_str() {
                "scene" | "group" => Mapped::Skip { reason: String::new() },
                _ => map_source(source),
            })
            .collect();
    }

    /// Give every source that became a GodwinMix source an id, and every OBS
    /// scene a document id, before any item needs to point at one.
    fn mint_ids(&mut self) {
        let mut taken: BTreeSet<String> = BTreeSet::new();
        for (index, source) in self.raw.sources.iter().enumerate() {
            if source.id == "scene" {
                self.scene_ids.insert(index, Id::new());
                continue;
            }
            if !matches!(self.mapped[index], Mapped::Core { .. } | Mapped::Plugin { .. }) {
                continue;
            }
            let base = slug(&source.name);
            let mut id = base.clone();
            let mut n = 2;
            while !taken.insert(id.clone()) {
                id = format!("{base}-{n}");
                n += 1;
            }
            self.ids.insert(index, id);
        }
    }

    /// Every OBS scene as a GodwinMix scene, in the order OBS lists them.
    fn build_scenes(&mut self) -> Vec<Scene> {
        let mut order: Vec<usize> = Vec::new();
        for named in self.raw.scene_order.clone() {
            if let Some(index) = self.by_name.get(&named.name) {
                if self.raw.sources[*index].id == "scene" {
                    order.push(*index);
                }
            }
        }
        for (index, source) in self.raw.sources.iter().enumerate() {
            if source.id == "scene" && !order.contains(&index) {
                order.push(index);
            }
        }
        let mut scenes = Vec::new();
        for index in order {
            let source = self.raw.sources[index].clone();
            let id = self.scene_ids[&index];
            let items = self.items_of(&source, &source.name);
            if !source.filters.is_empty() {
                self.notes.push(format!(
                    "the OBS scene {:?} had {} filter(s) on the scene itself; a filter over a whole composited scene is the expensive path here, so they were left out. Add them to the items that need them.",
                    source.name,
                    source.filters.len()
                ));
            }
            scenes.push(Scene { id, name: source.name.clone(), items, color: None });
        }
        scenes
    }

    /// The items of a scene or a group source.
    fn items_of(&mut self, source: &ObsSource, path: &str) -> Vec<Item> {
        let raw_items: Vec<ObsItem> = source
            .settings
            .get("items")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let mut items = Vec::new();
        for raw in &raw_items {
            if let Some(item) = self.build_item(raw, path) {
                items.push(item);
            }
        }
        items
    }

    /// One OBS scene item.
    fn build_item(&mut self, raw: &ObsItem, path: &str) -> Option<Item> {
        let index = self.resolve(raw)?;
        *self.placements.entry(index).or_default() += 1;
        let source = self.raw.sources[index].clone();
        let here = format!("{path} / {}", raw.name);
        let size = self.size_of(index, &source);
        let content = self.content_of(index, &source, &here)?;
        let (transform, crop) = self.geometry(raw, size, &here);
        let mut item = Item::new(content);
        item.name = Some(raw.name.clone());
        item.transform = transform;
        item.crop = crop;
        item.visible = raw.visible;
        item.locked = raw.locked;
        item.blend = blend_of(raw.blend_type.as_deref());
        item.filters = self.filters_of(&source, &here);
        // A group's transform is multiplied into its children here rather than
        // at apply time, so the numbers in the file are the numbers on the
        // canvas. That is the whole of OBS's group transform bug class.
        if let Content::Children { children } = &mut item.content {
            for child in children.iter_mut() {
                child.transform = compose(&item.transform, &child.transform);
                child.opacity *= item.opacity;
            }
            item.transform = Transform::default();
        }
        Some(item)
    }

    /// Which source an item points at: uuid first, then name (PR 8345's two
    /// identity systems, half unified).
    fn resolve(&mut self, raw: &ObsItem) -> Option<usize> {
        if let Some(uuid) = &raw.source_uuid {
            if let Some(index) = self.by_uuid.get(uuid) {
                return Some(*index);
            }
        }
        match self.by_name.get(&raw.name) {
            Some(index) => Some(*index),
            None => {
                self.notes.push(format!(
                    "the item {:?} names a source that is not in this collection, so it was left out. Export the collection again from OBS with every source present.",
                    raw.name
                ));
                None
            }
        }
    }

    /// What the item shows.
    fn content_of(&mut self, index: usize, source: &ObsSource, path: &str) -> Option<Content> {
        match source.id.as_str() {
            "scene" => {
                // A nested scene is a reference, which is what it always was.
                Some(Content::Ref { scene: *self.scene_ids.get(&index)?, overrides: BTreeMap::new() })
            }
            "group" => {
                if self.open_groups.contains(&index) {
                    self.notes.push(format!(
                        "the group {:?} contains itself, which OBS allows and nothing can draw, so it was left out.",
                        source.name
                    ));
                    return None;
                }
                self.open_groups.push(index);
                let children = self.items_of(source, path);
                self.open_groups.pop();
                Some(Content::Children { children })
            }
            _ => match &self.mapped[index] {
                Mapped::Core { .. } | Mapped::Plugin { .. } => {
                    Some(Content::Source { source: self.ids.get(&index)?.clone() })
                }
                Mapped::Graphic { graphic, params, .. } => {
                    Some(Content::Graphic { graphic: graphic.clone(), params: params.clone() })
                }
                Mapped::Skip { .. } => None,
            },
        }
    }

    /// The source's own pixel size, when anything in the file says what it is.
    fn size_of(&mut self, index: usize, source: &ObsSource) -> Option<(f64, f64)> {
        if let Some(size) = self.options.source_sizes.get(&source.name) {
            return Some(*size);
        }
        if source.id == "scene" {
            let custom = source.settings.get("custom_size").and_then(Value::as_bool) == Some(true);
            let (cx, cy) = (
                source.settings.get("cx").and_then(Value::as_f64),
                source.settings.get("cy").and_then(Value::as_f64),
            );
            return match (custom, cx, cy) {
                (true, Some(w), Some(h)) => Some((w, h)),
                _ => Some((self.canvas.width as f64, self.canvas.height as f64)),
            };
        }
        let width = source.settings.get("width").and_then(Value::as_f64);
        let height = source.settings.get("height").and_then(Value::as_f64);
        match (width, height) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Some((w, h)),
            _ => {
                let _ = index;
                None
            }
        }
    }

    /// Position, rotation, frame, fit and crop for one item.
    ///
    /// OBS crops the source's own pixels first, then either scales the result
    /// (no bounds) or fits it into a bounds box. Both orders are kept here, so
    /// an item lands on the same pixel it landed on in OBS.
    fn geometry(&mut self, raw: &ObsItem, size: Option<(f64, f64)>, path: &str) -> (Transform, Crop) {
        let mut transform = Transform {
            position: Vec2::new(raw.pos.x, raw.pos.y),
            rotation: raw.rot,
            scale: Vec2::ONE,
            anchor: anchor_of(raw.align),
            frame: None,
            fit: Fit::None,
            align: Align::Center,
        };
        let assumed = (self.canvas.width as f64, self.canvas.height as f64);
        let cropped = |size: (f64, f64)| {
            (
                (size.0 - raw.crop_left - raw.crop_right).max(1.0),
                (size.1 - raw.crop_top - raw.crop_bottom).max(1.0),
            )
        };
        match fit_of(&raw.bounds_type) {
            Some(fit) => {
                transform.frame = Some(Frame::new(raw.bounds.x, raw.bounds.y));
                transform.fit = fit;
                let inside = anchor_of(raw.bounds_align);
                transform.align = Align::from_factors(inside.x, inside.y);
            }
            None => match size {
                Some(size) => {
                    let (w, h) = cropped(size);
                    transform.frame = Some(Frame::new(w * raw.scale.x, h * raw.scale.y));
                    transform.fit = Fit::Stretch;
                }
                None => {
                    // Nothing in the file says how big the source is, so the
                    // item keeps its scale factors and the frame is left to the
                    // source's own size at apply time.
                    transform.scale = Vec2::new(raw.scale.x, raw.scale.y);
                    self.note_once(format!(
                        "nothing in the collection says how big the source behind {path} is, so its size is taken from the source itself when the scene is applied. Pass --source-size \"NAME=WIDTHxHEIGHT\" to place it exactly."
                    ));
                }
            },
        }
        let crop_size = size.unwrap_or(assumed);
        if size.is_none() && self.has_crop(raw) {
            self.note_once(format!(
                "the crop on {path} was measured against {}x{}, because the collection does not say how big that source is. Pass --source-size \"NAME=WIDTHxHEIGHT\" to get it exact.",
                assumed.0 as u32, assumed.1 as u32
            ));
        }
        let crop = normalise_crop(
            raw.crop_left,
            raw.crop_top,
            raw.crop_right,
            raw.crop_bottom,
            crop_size,
        );
        (transform, crop)
    }

    fn has_crop(&self, raw: &ObsItem) -> bool {
        raw.crop_left + raw.crop_top + raw.crop_right + raw.crop_bottom > 0.0
    }

    /// A source's filters, copied onto this placement.
    ///
    /// OBS attaches filters to sources, so a camera keyed in one scene is keyed
    /// in all of them. Here a filter belongs to the placement, which is the
    /// better model and the one thing an import cannot do silently: the copies
    /// are independent from now on, and the report says which they were.
    fn filters_of(&mut self, source: &ObsSource, path: &str) -> Vec<Filter> {
        let mut out = Vec::new();
        for obs in &source.filters {
            let kind = FILTER_TYPES
                .iter()
                .find(|(id, _)| *id == obs.id)
                .map(|(_, kind)| (*kind).to_string())
                .unwrap_or_else(|| format!("obs:{}", obs.id));
            if kind.starts_with("obs:") {
                self.note_once(format!(
                    "the filter {:?} on {:?} is an OBS filter type ({}) with no GodwinMix equivalent; it is in the document under its OBS name so nothing is lost, and it will not run until a plugin claims it.",
                    obs.name, source.name, obs.id
                ));
            }
            out.push(Filter {
                kind: kind.clone(),
                name: Some(obs.name.clone()),
                enabled: obs.enabled,
                params: obs.settings.clone(),
            });
            match self.filters.iter_mut().find(|f| f.filter == obs.name && f.source == source.name)
            {
                Some(report) => report.placements.push(path.to_string()),
                None => self.filters.push(FilterReport {
                    filter: obs.name.clone(),
                    obs_type: obs.id.clone(),
                    source: source.name.clone(),
                    placements: vec![path.to_string()],
                }),
            }
        }
        out
    }

    /// Add a note unless the same one is already there. The same thing wrong
    /// with twelve items is one line, not twelve.
    fn note_once(&mut self, note: String) {
        if !self.notes.contains(&note) {
            self.notes.push(note);
        }
    }

    /// One report line per OBS source, scenes and groups included.
    fn source_reports(&self) -> Vec<SourceReport> {
        let mut out = Vec::new();
        for (index, source) in self.raw.sources.iter().enumerate() {
            let placements = self.placements.get(&index).copied().unwrap_or(0);
            let outcome = match (&source.id[..], &self.mapped[index]) {
                ("scene", _) => Outcome::Imported {
                    r#type: "scene".into(),
                    id: self.scene_ids[&index].to_string(),
                },
                ("group", _) => {
                    Outcome::Imported { r#type: "group".into(), id: slug(&source.name) }
                }
                (_, Mapped::Core { kind, .. }) => Outcome::Imported {
                    r#type: kind.clone(),
                    id: self.ids.get(&index).cloned().unwrap_or_default(),
                },
                (_, Mapped::Plugin { kind, plugin, .. }) => Outcome::NeedsPlugin {
                    r#type: kind.clone(),
                    id: self.ids.get(&index).cloned().unwrap_or_default(),
                    plugin: plugin.clone(),
                },
                (_, Mapped::Graphic { graphic, note, .. }) => Outcome::Skipped {
                    reason: format!(
                        "GodwinMix has no text source; text is a graphic here, and no graphic plugin is installed yet"
                    ),
                    placeholder: Some(format!("{note} ({graphic})")),
                },
                (_, Mapped::Skip { reason }) => {
                    Outcome::Skipped { reason: reason.clone(), placeholder: None }
                }
            };
            out.push(SourceReport {
                obs_type: source.versioned_id.clone().unwrap_or_else(|| source.id.clone()),
                obs_name: source.name.clone(),
                outcome,
                placements,
            });
        }
        out
    }

    /// The config entries for every source that became one.
    fn imported_sources(&self) -> Vec<ImportedSource> {
        let mut out = Vec::new();
        for (index, id) in &self.ids {
            let source = &self.raw.sources[*index];
            let (kind, uri, params) = match &self.mapped[*index] {
                Mapped::Core { kind, uri, params } => (kind.clone(), uri.clone(), params.clone()),
                Mapped::Plugin { kind, params, .. } => (kind.clone(), None, params.clone()),
                _ => continue,
            };
            out.push(ImportedSource {
                id: id.clone(),
                name: source.name.clone(),
                kind,
                uri,
                params: if params.is_null() { json!({}) } else { params },
            });
        }
        out
    }
}
