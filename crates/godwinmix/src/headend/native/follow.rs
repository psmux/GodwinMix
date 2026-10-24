//! The followed Flussonic file and the channels kept here, side by side.
//!
//! A stream of the file is the file's until it is edited or removed on the
//! headend screen. Then the station takes it over: it is kept here, its
//! show is handed from the file's keeper to this one without a restart, and
//! the file's later edits to it are not applied. `headend.channel.follow`
//! gives it back to the file, which then makes it as the file says.

use super::compile;
use super::Native;
use crate::headend::state::Headend;
use crate::station::state::Station;
use godwinmix_flussonic::import::{self, ShowPlan};
use godwinmix_protocol::headend::HeadendChannel;
use std::sync::Arc;

/// The headend that follows a file, when there is one.
pub fn headend(st: &Station) -> Option<Arc<Headend>> {
    st.headend.get().and_then(std::sync::Weak::upgrade).filter(|h| h.file.is_some())
}

/// The file's streams that are still the file's, as plans, with the file.
pub fn file_plans(st: &Station, n: &Native) -> Option<(String, Vec<ShowPlan>)> {
    let h = headend(st)?;
    let owned = n.owned();
    let plans = import::import(&h.doc.lock().config).shows.into_iter().filter(|p| !owned.contains(&p.name)).collect();
    Some((h.file_text(), plans))
}

/// A stream of the file as a channel, to edit or to show.
pub fn file_channel(st: &Station, n: &Native, name: &str) -> Option<(String, HeadendChannel)> {
    let (file, plans) = file_plans(st, n)?;
    let p = plans.into_iter().find(|p| p.name == name)?;
    let profiles = compile::all_profiles(&n.store.lock().profiles);
    let c = compile::channel_of(&p, &profiles, &format!("{file}, edited here"));
    Some((file, c))
}

/// Take stream `name` out of the file's hands: from now on the store holds
/// it, and its running show is this keeper's. The caller saves the store.
pub fn detach(st: &Station, n: &Native, name: &str, file: &str) {
    n.store.lock().detached.insert(name.to_string(), file.to_string());
    if let Some(h) = headend(st) {
        if let Some(a) = h.keep.take(name) {
            n.keep.give(name, a);
        }
    }
}

/// Give stream `name` back to the file. False when it was never taken.
pub fn give_back(st: &Station, n: &Native, name: &str) -> bool {
    let was = {
        let mut s = n.store.lock();
        let was = s.detached.remove(name).is_some();
        if was {
            s.channels.retain(|c| c.name != name);
        }
        was
    };
    if was {
        if let (Some(h), Some(a)) = (headend(st), n.keep.take(name)) {
            h.keep.give(name, a);
        }
    }
    was
}

/// Apply the file again, after the store took a stream or gave one back.
pub async fn reapply(st: &Station) {
    if let Some(h) = headend(st) {
        crate::headend::apply::apply_file(&h, false).await;
    }
}
