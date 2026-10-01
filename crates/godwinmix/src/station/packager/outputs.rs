//! Every HLS output this process packages, by show and output id, made to
//! match what the station last handed over.

use super::output::Packager;
use super::wire::{Report, Want};
use godwinmix_core::hls::Stream;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Default)]
pub struct Outputs {
    running: Mutex<BTreeMap<(String, String), Running>>,
}

struct Running {
    want: Want,
    packager: Packager,
}

impl Outputs {
    /// Start what is new, stop what went, and start again what now reads
    /// something else. Starting is a thread spawn and stopping sets a flag,
    /// so this never waits on a pipeline.
    pub fn apply(&self, wanted: Vec<Want>) {
        let wanted: BTreeMap<_, _> = wanted.into_iter().map(|w| ((w.show.clone(), w.output.clone()), w)).collect();
        let mut running = self.running.lock();
        running.retain(|k, _| wanted.contains_key(k));
        for (key, want) in wanted {
            if running.get(&key).is_some_and(|r| r.want == want) {
                continue;
            }
            // A stream already published keeps its rings, so a player sees
            // one discontinuity and not a new stream.
            let stream = match running.get(&key) {
                Some(r) if r.want.params() == want.params() && r.want.viewer_key == want.viewer_key => r.packager.stream.clone(),
                _ => Arc::new(Stream::new(&want.output, want.params(), &want.viewer_key)),
            };
            let sound = refusal(&want.show, &want.output);
            let packager = Packager::start(stream, want.source.clone(), want.why_not.clone(), sound);
            running.insert(key, Running { want, packager });
        }
    }

    pub fn stream(&self, show: &str, output: &str) -> Option<Arc<Stream>> {
        self.running.lock().get(&(show.to_string(), output.to_string())).map(|r| r.packager.stream.clone())
    }

    /// The HLS outputs one show serves now.
    pub fn ids(&self, show: &str) -> Vec<String> {
        self.running.lock().keys().filter(|(s, _)| s == show).map(|(_, o)| o.clone()).collect()
    }

    /// What every output is doing.
    pub fn reports(&self) -> Vec<Report> {
        let running = self.running.lock();
        running
            .iter()
            .map(|((show, output), r)| Report {
                show: show.clone(),
                output: output.clone(),
                live: r.packager.board.read(),
                viewers: r.packager.stream.viewers.count() as u32,
            })
            .collect()
    }
}

/// The sentence for sound fragmented MP4 does not carry, naming the call
/// that fixes it.
fn refusal(show: &str, output: &str) -> impl Fn(&str) -> String + Send + 'static {
    let (show, output) = (show.to_string(), output.to_string());
    move |codec| {
        format!(
            "the input's sound is {codec}, and HLS carries AAC: copying it would make segments no player can play. \
             Give the output a rendition with AAC sound, show.output.set {{id: \"{show}\", output: \"{output}\", \
             rendition: {{\"audio\": {{\"codec\": \"aac\"}}}}}}, and the picture is still copied."
        )
    }
}
