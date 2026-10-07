//! Planning runs on every change to an output, so it has to be cheap. A show
//! with 16 sources and 64 requests must plan in well under a millisecond.
//! Run with `--nocapture` to see the number; `--release` for the real one.

mod common;

use std::time::Instant;

use common::random::Rng;
use common::*;
use godwinmix_render::*;

#[test]
fn sixteen_sources_and_sixty_four_requests_plan_in_well_under_a_millisecond() {
    let model = software().with_hardware("h264-gpu", VideoCodec::H264, "gpu0");
    let mut rng = Rng(42);
    let (srcs, mut reqs) = rng.show(16, 64);
    // A refused request would stop the plan early and flatter the number.
    for (_, r) in &mut reqs {
        if let Some(v) = r
            .video
            .as_mut()
            .filter(|v| v.codec == Some(VideoCodec::Vp8))
        {
            v.codec = None;
        }
    }
    assert_eq!(reqs.len(), 64);
    let first = plan(&srcs, &reqs, &model).expect("the timing show plans");
    assert!(first.nodes.len() > 64, "{} nodes", first.nodes.len());

    let runs = 200;
    let start = Instant::now();
    for _ in 0..runs {
        let p = plan(&srcs, &reqs, &model).unwrap();
        std::hint::black_box(p);
    }
    let each = start.elapsed() / runs;
    eprintln!(
        "planned {} requests on {} sources into {} nodes ({} encodes) in {each:?} each",
        reqs.len(),
        srcs.len(),
        first.nodes.len(),
        first.encodes().count()
    );
    // Debug builds are several times slower than release; this bound holds
    // for both on a laptop. Times GODWINMIX_TIMING_SLACK on a runner that
    // says it is slow: a shared Linux runner took 1.096 ms with the rest of
    // the suite beside it.
    let slack = std::env::var("GODWINMIX_TIMING_SLACK").ok().and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s >= 1.0).unwrap_or(1.0);
    assert!((each.as_micros() as f64) < 1000.0 * slack, "planning took {each:?}");
}

#[test]
fn diffing_two_large_plans_is_cheap_too() {
    let model = software();
    let (srcs, reqs) = Rng(7).show(16, 64);
    let reqs: Vec<_> = reqs
        .into_iter()
        .filter(|(_, r)| r.video.as_ref().and_then(|v| v.codec) != Some(VideoCodec::Vp8))
        .collect();
    let a = plan(&srcs, &reqs, &model).unwrap();
    let b = plan(&srcs, &reqs[1..], &model).unwrap();
    let start = Instant::now();
    for _ in 0..200 {
        std::hint::black_box(diff(&a, &b));
    }
    let each = start.elapsed() / 200;
    eprintln!(
        "diffed {} against {} nodes in {each:?}",
        a.nodes.len(),
        b.nodes.len()
    );
    assert!(each.as_micros() < 1000, "diff took {each:?}");
}
