//! Which queues hold the most, and whose source each one is.

use crate::observe::introspect;
use gstreamer as gst;
use gstreamer::prelude::*;

const MB: u64 = 1024 * 1024;
/// How many queues the log names.
const NAMED: usize = 5;

/// One queue and what it holds.
#[derive(Debug, Clone)]
pub struct Held {
    pub pipeline: String,
    pub element: String,
    pub bytes: u64,
    pub source: Option<String>,
}

/// The queues the log names, fullest first: `element in pipeline: N MB`.
pub(super) fn describe(queues: &[Held]) -> String {
    let named: Vec<String> = queues
        .iter()
        .take(NAMED)
        .map(|q| format!("{} in {}: {} MB", q.element, q.pipeline, q.bytes / MB))
        .collect();
    if named.is_empty() {
        "no queue holds anything".into()
    } else {
        named.join(", ")
    }
}

/// Every queue in every pipeline the show has, fullest by bytes first.
pub fn fullest_queues() -> Vec<Held> {
    let mut out = Vec::new();
    for name in introspect::names() {
        let Some(pipeline) = introspect::pipeline(&name) else { continue };
        for element in pipeline.iterate_recurse().into_iter().flatten() {
            let factory = element.factory().map(|f| f.name().to_string()).unwrap_or_default();
            if factory != "queue" {
                continue;
            }
            let bytes = u64::from(element.property::<u32>("current-level-bytes"));
            let source = source_of(&name, &element);
            out.push(Held { pipeline: name.clone(), element: element.name().to_string(), bytes, source });
        }
    }
    out.sort_by_key(|q| std::cmp::Reverse(q.bytes));
    out
}

/// The source a queue belongs to, if it belongs to one.
///
/// A source's own pipeline is called `input-<id>`. In the programme, its
/// branch begins at `pgm-vsrc-<id>` and `pgm-asrc-<id>`, through `pgm-vq-<id>`
/// and `pgm-vtee-<id>`, and a slot's queue hangs off that tee through its
/// gate; so the chain above a queue is walked a few elements up until one of
/// those names, or a proxy source's own inner queue's parent, says whose it is.
pub fn source_of(pipeline: &str, queue: &gst::Element) -> Option<String> {
    if let Some(id) = pipeline.strip_prefix("input-") {
        return Some(id.to_string());
    }
    if pipeline != introspect::PROGRAMME {
        return None;
    }
    let mut at = queue.clone();
    for _ in 0..8 {
        if let Some(id) = branch_id(&at.name()) {
            return Some(id);
        }
        if let Some(parent) = at.parent().and_then(|p| branch_id(&p.name())) {
            return Some(parent);
        }
        let pad = at.sink_pads().into_iter().next()?.peer()?;
        at = pad.parent_element()?;
    }
    None
}

/// The source id in the name of an element of a source's programme branch.
fn branch_id(name: &str) -> Option<String> {
    ["pgm-vq-", "pgm-aq-", "pgm-vtee-", "pgm-vsrc-", "pgm-asrc-"]
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

