//! Where a source's frames wait, for the `other` column of the source rows:
//! the queues holding buffers, what the compositor says it waits, and the
//! threads each video decoder was given.

use godwinmix_core::observe::{introspect, PROGRAMME};
use gstreamer::prelude::*;

/// One line for a source called `id`.
pub fn line(id: &str) -> String {
    format!("queued buffers: {}; {}; {}", queued(id), compositor_latency(), decoders(id))
}

/// Every queue holding a buffer on the programme and in the source's own
/// pipeline.
fn queued(id: &str) -> String {
    let mut held: Vec<String> = Vec::new();
    for name in [PROGRAMME.to_string(), format!("input-{id}")] {
        for q in introspect::queues(&name).unwrap_or_default().into_iter().filter(|q| q.buffers > 0) {
            held.push(format!("{} {}", q.element, q.buffers));
        }
    }
    if held.is_empty() {
        "none".into()
    } else {
        held.join(", ")
    }
}

/// What the compositor says it waits, which is how much of every input sits
/// on its pads.
fn compositor_latency() -> String {
    let report = introspect::latency(PROGRAMME);
    let vmix = report.ok().and_then(|r| r.stages.into_iter().find(|s| s.element == "vmix"));
    match vmix {
        Some(s) => format!("compositor latency {:.0} ms", s.min_ms),
        None => "compositor latency unknown".into(),
    }
}

/// The source's decoders and the threads each was given.
fn decoders(id: &str) -> String {
    let Some(pipeline) = introspect::pipeline(&format!("input-{id}")) else {
        return "no source pipeline".into();
    };
    let found: Vec<String> = pipeline
        .iterate_recurse()
        .into_iter()
        .flatten()
        .filter(|e| e.factory().is_some_and(|f| f.klass().contains("Decoder/Video")))
        .map(|e| match e.find_property("max-threads") {
            Some(_) => {
                let threads = e.property_value("max-threads").serialize().unwrap_or_default();
                format!("{} max-threads {threads}", e.name())
            }
            None => e.name().to_string(),
        })
        .collect();
    if found.is_empty() {
        "no video decoder".into()
    } else {
        format!("decoders: {}", found.join(", "))
    }
}
