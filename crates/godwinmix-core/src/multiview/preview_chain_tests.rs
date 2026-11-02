//! A chain that fails halfway, on a real pipeline: the first filter goes
//! in, the second builds a bin with no src pad and cannot be linked. The
//! item must be left drawn clean, flip straight to pad, with no bin left
//! behind. It used to be left linked to nothing, so the item fell out of
//! Preview and the mosaic's bus had a not linked flow on it.

use super::*;
use crate::config::Params;
use crate::plugin::filter::Filter;
use crate::plugin::Manifest;

/// Builds a bin with a sink and no src, which no link can get through.
struct NoWayOut;

impl Filter for NoWayOut {
    fn manifest(&self) -> &Manifest {
        &crate::plugin::filters::colour::MANIFEST
    }

    fn build(&mut self, _canvas: &CanvasCaps, _params: &Params) -> Result<gst::Element> {
        let bin = gst::Bin::with_name("filter-no-way-out");
        let id = crate::gstutil::make("identity", "filter-no-way-out-id")?;
        bin.add(&id)?;
        bin.add_pad(&gst::GhostPad::with_target(&id.static_pad("sink").unwrap())?)?;
        Ok(bin.upcast())
    }

    fn configure(&mut self, _params: &Params) -> Result<Configure> {
        Ok(Configure::Applied)
    }
}

fn spec(type_id: &str) -> FilterSpec {
    FilterSpec { id: "t".into(), type_id: type_id.into(), side: FilterSide::SceneItem, params: Params::new() }
}

fn want(type_id: &str) -> ItemFilter {
    ItemFilter { type_id: type_id.into(), name: None, params: Params::new() }
}

#[test]
fn a_chain_that_fails_halfway_leaves_the_flip_linked_to_the_pad() {
    gst::init().unwrap();
    let canvas = CanvasCaps::new(&crate::config::Canvas {
        width: 320,
        height: 180,
        fps: crate::config::FrameRate::whole(15),
        sample_rate: 48000,
        channels: 2,
    });
    let pipeline = gst::Pipeline::new();
    let flip = crate::gstutil::make("videoflip", "t-flip").unwrap();
    let comp = crate::gstutil::make("compositor", "t-comp").unwrap();
    pipeline.add_many([&flip, &comp]).unwrap();
    let pad = comp.request_pad_simple("sink_%u").unwrap();
    flip.static_pad("src").unwrap().link(&pad).unwrap();
    let board = Transparent::new(&comp, (320, 180));

    let colour = Built::new(spec("color/filter"), filter::make("color/filter").unwrap(), &canvas).unwrap();
    let broken = Built::new(spec("no-way-out"), Box::new(NoWayOut), &canvas).unwrap();
    let at = At { pipeline: &pipeline, flip: &flip, pad: &pad, source: "cam1", serial: 1, key: "cam1".into() };
    let chain = Chain::build(&at, &[want("color/filter"), want("no-way-out")], Ok(vec![colour, broken]), &board);

    assert!(chain.elements().is_empty(), "a chain that failed kept a filter");
    let peer = flip.static_pad("src").unwrap().peer();
    assert_eq!(peer.as_ref(), Some(&pad), "the flip was left linked to {:?}, not the pad", peer.as_ref().map(|p| p.name()));
    let left: Vec<String> = pipeline
        .iterate_elements()
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.name().to_string())
        .filter(|n| n.starts_with("filter-"))
        .collect();
    assert!(left.is_empty(), "filter bins left in the pipeline: {left:?}");
}
