//! Saying a source's state when it changes, every change.
//!
//! `event/source.state` was only ever sent for two of the four states: the
//! `connecting` said as a source is added, and the `failed` said when its
//! pipeline posts an error. `live` and `stalled` were left to the next full
//! status snapshot, and nothing sends one when a source starts delivering. A
//! client following the events therefore had a source at `connecting` for as
//! long as nothing else happened, which on 0.2.2 was the preview pane writing
//! "connecting, no picture yet" over a clip that had been playing for minutes,
//! its scrubber moving in the Sources tile beside it. A hook on `source.state`
//! and the `gmx_source_state` gauge had the same hole.
//!
//! So each tick compares what each source is with what was last said about
//! it, and says the difference. One comparison per source per tick, and an
//! event only on a change.

use super::Mixer;
use crate::state::Event;

impl Mixer {
    pub(super) fn report_states(&mut self) {
        for slot in &mut self.sources {
            let now = slot.input.observed_state();
            if now == slot.reported {
                continue;
            }
            slot.reported = now;
            let _ = self.events.send(Event::SourceStateChanged { source: slot.input.id.clone(), state: now });
        }
    }
}
