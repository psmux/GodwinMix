//! An item whose filter will not go on is held hidden, and is drawn on the
//! apply that gets the filter in. Before, it was drawn raw and the failure
//! remembered, so a cutout camera whose cutout could not be made in the
//! second after a restart put the whole room on air.

use super::*;

fn item_with(canvas: &crate::caps::CanvasCaps, item: crate::scene::id::Id, filter: &str) -> Placement {
    let mut p = Placement::full_canvas("cam1".into(), canvas);
    p.item = Some(item);
    p.filters = vec![slots::ItemFilter { type_id: filter.into(), name: None, params: Default::default() }];
    p
}

fn shown(mix: &Mixer) -> bool {
    mix.pool.slots().iter().any(|s| s.showing())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_whose_filter_will_not_go_on_stays_hidden_until_it_does() {
    let mut mix = with_sources(&["cam1"]).await;
    let canvas = mix.canvas.clone();
    let item = crate::scene::id::Id::new();

    mix.take_scene(scene("set", vec![item_with(&canvas, item, "no-such/filter")]), None).expect("the take goes ahead");
    assert!(!shown(&mix), "the item was drawn without the filter it asks for");
    // The visibility tick applies again, and tries the filter again.
    mix.apply_visibility(false);
    assert!(!shown(&mix), "a second apply drew it raw");
    assert!(mix.pool.item_filters().is_empty(), "half a chain was left on");

    // The same item, its filter now one this build can make: drawn, keyed.
    mix.take_scene(scene("set", vec![item_with(&canvas, item, "chroma/filter")]), None).expect("the take goes ahead");
    assert!(shown(&mix), "the item stayed hidden with its filter in");
    assert_eq!(mix.pool.item_filters().len(), 1);
    mix.shutdown();
}
