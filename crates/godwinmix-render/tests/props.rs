//! Property tests over random shows. A small xorshift stands in for a
//! property testing crate: the properties are few and the inputs are easy
//! to generate, so a dependency would not earn its place.

mod common;

use std::collections::HashSet;

use common::random::Rng;
use common::*;
use godwinmix_render::*;

const CASES: u64 = 500;

fn each_random_plan(check: impl Fn(&Plan, &[(SourceId, RenditionRequest)])) {
    let model = software()
        .with_hardware("h264-gpu", VideoCodec::H264, "gpu0")
        .with_room(
            "gpu0",
            Room {
                sessions: Some(3),
                device_millis: None,
            },
        );
    let mut planned = 0;
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let (n_sources, n_requests) = (1 + rng.below(6) as usize, 1 + rng.below(20) as usize);
        let (srcs, reqs) = rng.show(n_sources, n_requests);
        match plan(&srcs, &reqs, &model) {
            Ok(p) => {
                planned += 1;
                check(&p, &reqs);
            }
            Err(e) => assert!(!e.to_string().is_empty()),
        }
    }
    assert!(
        planned > CASES * 3 / 4,
        "only {planned} of {CASES} random shows planned"
    );
}

#[test]
fn no_two_encode_nodes_do_the_same_work() {
    each_random_plan(|p, _| {
        let mut seen = HashSet::new();
        for node in p.encodes() {
            let NodeKind::Encode { source, shape, .. } = &node.kind else {
                unreachable!()
            };
            assert!(
                seen.insert((source.clone(), *shape)),
                "{} twice in {:#?}",
                node.id,
                p.nodes
            );
        }
        let mut audio = HashSet::new();
        for node in &p.nodes {
            if let NodeKind::AudioEncode { source, shape } = &node.kind {
                assert!(audio.insert((source.clone(), *shape)), "{} twice", node.id);
            }
        }
    });
}

#[test]
fn no_source_is_decoded_twice() {
    each_random_plan(|p, _| {
        let mut seen = HashSet::new();
        for node in &p.nodes {
            if let NodeKind::Decode { source, track } = &node.kind {
                assert!(seen.insert((source.clone(), *track)), "{} twice", node.id);
            }
        }
    });
}

#[test]
fn every_request_has_one_mux_and_every_encode_of_a_source_one_interval() {
    each_random_plan(|p, reqs| {
        for (_, req) in reqs {
            assert!(p.output(&req.id).is_some(), "no mux for {}", req.id);
        }
        for node in p.encodes() {
            let NodeKind::Encode { source, shape, .. } = &node.kind else {
                unreachable!()
            };
            assert_eq!(shape.keyframe_ms, p.keyframe_ms[source]);
        }
    });
}

#[test]
fn ids_are_unique_and_inputs_come_first() {
    each_random_plan(|p, _| {
        let mut seen = HashSet::new();
        for node in &p.nodes {
            for input in &node.inputs {
                assert!(
                    seen.contains(input.as_str()),
                    "{} reads {input} before it starts",
                    node.id
                );
            }
            assert!(seen.insert(node.id.as_str()), "{} twice", node.id);
        }
    });
}

#[test]
fn the_gpu_never_holds_more_sessions_than_it_has() {
    each_random_plan(|p, _| {
        let held = p.cost.get("gpu0").map_or(0, |c| c.device_sessions);
        assert!(held <= 3, "{held} sessions on a three session GPU");
    });
}

#[test]
fn a_plan_diffed_against_itself_is_empty_and_is_deterministic() {
    let model = software();
    for seed in 1..=100 {
        let mut rng = Rng(seed);
        let (srcs, reqs) = rng.show(4, 12);
        let (Ok(a), Ok(b)) = (plan(&srcs, &reqs, &model), plan(&srcs, &reqs, &model)) else {
            continue;
        };
        assert_eq!(a, b);
        assert!(diff(&a, &b).is_empty());
    }
}
