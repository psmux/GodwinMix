//! An item with filters is not drawn until they are in.
//!
//! A cutout camera drawn before its cutout exists is the whole room over the
//! set. In the second after a restart that happened: the filter could not be
//! built on the first apply, the slot was drawn anyway with the camera raw,
//! and the failure was remembered as done, so nothing built it again until
//! the item changed.
//!
//! Now a placement whose filters are not all on its chain is drawn at alpha 0
//! and its slot marked `waiting`, so a transition or an item's own entrance
//! that would raise the pad aims at 0 too. The build is tried again on the
//! next apply (the visibility tick applies twice a second), and the item
//! appears on the apply that gets its filters in. A filter that can never be
//! built leaves the item hidden: `filter.add` refuses what this build cannot
//! make, and the reason is said once here, not twice a second.

use super::{Placement, SlotPool};
use std::borrow::Cow;
use tracing::warn;

impl SlotPool {
    /// Build the slot's filters for this placement. The placement to draw:
    /// itself when its filters are in, at alpha 0 when they are not.
    pub(super) fn filters_in<'p>(&mut self, index: usize, p: &'p Placement) -> Cow<'p, Placement> {
        let built = self.sync_filters(index, p);
        let slot = &mut self.slots[index];
        match built {
            Ok(()) => {
                slot.waiting = false;
                slot.refused = None;
                Cow::Borrowed(p)
            }
            Err(e) => {
                slot.waiting = true;
                if slot.refused.as_deref() != Some(p.filters.as_slice()) {
                    warn!(slot = index, source = %p.source, ?e, "a scene item's filter could not go on; the item stays hidden until it does");
                    slot.refused = Some(p.filters.clone());
                }
                Cow::Owned(Placement { alpha: 0.0, ..p.clone() })
            }
        }
    }

    /// The alpha a transition or an entrance may raise this slot to: none
    /// while its filters are not in.
    pub(super) fn may_show(&self, index: usize, alpha: f64) -> f64 {
        if self.slots[index].waiting {
            0.0
        } else {
            alpha
        }
    }
}
