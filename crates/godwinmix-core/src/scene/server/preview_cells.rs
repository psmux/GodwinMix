//! The armed scene's placements as the preview draws them.
//!
//! One function for the armed scene and for a draft shown in its place, so
//! the two cannot drift. Each cell carries its item's filters, because the
//! preview has to show a keyed presenter keyed: it used to carry only the
//! geometry, and Preview showed the green screen that Programme keyed out.
//!
//! A group with a filter is one placement on the programme, composited on its
//! own and filtered as one picture (`mixer::group`). The preview has no sub
//! compositor and skipped the group altogether. It now draws the group's
//! children with the group's filters after their own, which is the same
//! picture for every filter that works pixel by pixel (a look, a colour
//! correction) and for a key over children that do not overlap.

use super::{PreviewCell, Placement};

/// `placements` in canvas pixels, scaled by `sx` and `sy`.
pub(super) fn cells(placements: Vec<Placement>, sx: f64, sy: f64) -> Vec<PreviewCell> {
    let mut out = Vec::with_capacity(placements.len());
    for p in placements {
        if p.group.is_empty() {
            out.push(cell(p, sx, sy));
            continue;
        }
        let outer = p.filters;
        for mut kid in p.group {
            kid.filters.extend(outer.iter().cloned());
            out.push(cell(kid, sx, sy));
        }
    }
    out
}

fn cell(p: Placement, sx: f64, sy: f64) -> PreviewCell {
    PreviewCell {
        source: p.source,
        x: (p.xpos as f64 * sx).round() as i32,
        y: (p.ypos as f64 * sy).round() as i32,
        width: (p.width as f64 * sx).round() as i32,
        height: (p.height as f64 * sy).round() as i32,
        alpha: p.alpha,
        rotation: p.rotation,
        crop: p.crop,
        fit: p.sizing.name().to_string(),
        filters: p.filters,
    }
}

#[cfg(test)]
mod tests {
    use crate::caps::CanvasCaps;
    use crate::scene::document::{Content, Filter, Frame, Item, Scene};
    use crate::scene::server::SceneServer;
    use serde_json::json;

    fn key() -> Filter {
        Filter { kind: "chroma/filter".into(), name: None, enabled: true, params: json!({ "similarity": 0.3 }) }
    }

    fn look() -> Filter {
        Filter { kind: "lut/filter".into(), name: None, enabled: true, params: json!({ "lut": "black-and-white" }) }
    }

    fn server() -> std::sync::Arc<SceneServer> {
        SceneServer::in_memory(CanvasCaps::new(&crate::config::Canvas {
            width: 1920,
            height: 1080,
            fps: crate::config::FrameRate::whole(30),
            sample_rate: 48000,
            channels: 2,
        }))
    }

    /// The bug: the cells carried the geometry and nothing else, so the
    /// preview compositor never heard of a filter.
    #[test]
    fn an_armed_item_and_a_shown_draft_carry_their_filters_to_the_preview() {
        let s = server();
        s.edit(None, |doc| {
            let mut scene = Scene::new("studio");
            scene.items.push(Item::new(Content::Source { source: "set".into() }));
            let mut presenter = Item::new(Content::Source { source: "presenter".into() });
            presenter.filters.push(key());
            presenter.filters.push(Filter { enabled: false, ..look() });
            scene.items.push(presenter);
            doc.scenes.push(scene);
            Ok(())
        })
        .unwrap();
        s.arm(Some("studio")).unwrap();
        let cells = s.preview_layout(320, 180).unwrap().cells;
        assert!(cells[0].filters.is_empty());
        let kinds: Vec<_> = cells[1].filters.iter().map(|f| f.type_id.as_str()).collect();
        assert_eq!(kinds, ["chroma/filter"], "a disabled filter is not drawn, as on air");
        assert_eq!(cells[1].filters[0].params.get("similarity").and_then(|v| v.as_float()), Some(0.3));

        // The composer's draft, with a look added: Preview has it at once.
        let draft = s.edit_begin(None, "studio", false).unwrap().id.to_string();
        s.show_draft(Some(&draft)).unwrap();
        s.edit_draft(&draft, |doc, i| {
            doc.scenes[i].items[1].filters.push(look());
            Ok(())
        })
        .unwrap();
        let kinds: Vec<_> =
            s.preview_layout(320, 180).unwrap().cells[1].filters.iter().map(|f| f.type_id.clone()).collect();
        assert_eq!(kinds, ["chroma/filter", "lut/filter"]);
    }

    /// A group with a filter was left out of the preview altogether. Its
    /// children are drawn now, each with its own filters and then the group's.
    #[test]
    fn a_filtered_group_is_drawn_as_its_children_with_the_groups_filter_after_their_own() {
        let s = server();
        s.edit(None, |doc| {
            let mut scene = Scene::new("graded");
            let mut a = Item::new(Content::Source { source: "cam1".into() });
            a.transform.frame = Some(Frame::new(960.0, 1080.0));
            a.filters.push(key());
            let mut b = Item::new(Content::Source { source: "cam2".into() });
            b.transform.position.x = 960.0;
            b.transform.frame = Some(Frame::new(960.0, 1080.0));
            let mut group = Item::new(Content::Children { children: vec![a, b] });
            group.filters.push(look());
            scene.items.push(group);
            doc.scenes.push(scene);
            Ok(())
        })
        .unwrap();
        s.arm(Some("graded")).unwrap();
        let cells = s.preview_layout(320, 180).unwrap().cells;
        let drawn: Vec<(String, Vec<String>)> = cells
            .iter()
            .map(|c| (c.source.clone(), c.filters.iter().map(|f| f.type_id.clone()).collect()))
            .collect();
        assert_eq!(
            drawn,
            [
                ("cam1".to_string(), vec!["chroma/filter".to_string(), "lut/filter".to_string()]),
                ("cam2".to_string(), vec!["lut/filter".to_string()]),
            ]
        );
        assert_eq!(cells[1].x, 160, "the children keep their places");
    }
}
