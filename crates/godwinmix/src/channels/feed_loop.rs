//! Keeping the shows' outputs in step with the channels the programme feeds.
//!
//! Whoever owns the shows hands over an [`Asker`]: the station, which asks
//! each show over its socket, or a single process core, which asks its own
//! mixer. A task then looks after the `to-channel-` outputs: at start, when
//! a channel changes, and every fifteen seconds while any channel is fed by
//! the programme, so a show that restarted or an output somebody removed is
//! put right without a person. With no programme feed and nothing of its
//! own left in any show it asks nothing at all.
//!
//! An output is kept by its show across a restart, in the show's runtime
//! store, so a restart finds it in place; it is given its address once more
//! (`plan::steps`) and never added twice.

mod core;
mod plan;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use godwinmix_protocol::error::RpcError;
use parking_lot::Mutex;
use serde_json::Value;

pub use self::core::OwnMixer;
use super::feed::ProgrammeFeed;
use super::Channels;

/// How often the outputs are looked at while any channel needs them.
const EVERY: Duration = Duration::from_secs(15);

pub type Answer<'a> = Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send + 'a>>;

/// The shows, as whoever owns them reaches them.
pub trait Asker: Send + Sync + 'static {
    /// The show a feed naming none goes to.
    fn first(&self) -> String;
    /// Every show that runs a programme: its id, and why it cannot be asked
    /// now when it cannot.
    fn shows(&self) -> Vec<(String, Option<String>)>;
    /// One call to one show.
    fn ask<'a>(&'a self, show: &'a str, method: &'a str, params: Value) -> Answer<'a>;
}

/// The task's own state, kept with the channels.
#[derive(Default)]
pub struct Loop {
    poke: tokio::sync::Notify,
    /// `(show, output)` to the address last given to it by this process.
    given: Mutex<HashMap<(String, String), String>>,
    /// Nothing wanted, and nothing of ours left in any show.
    clean: AtomicBool,
}

impl Channels {
    /// Start looking after the programme's outputs. Once per process.
    pub fn run_programme_feeds(self: &Arc<Self>, asker: Arc<dyn Asker>) {
        let lp = Arc::new(Loop::default());
        if self.feed_loop.set(lp.clone()).is_err() {
            return;
        }
        let me = Arc::downgrade(self);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            while let Some(channels) = me.upgrade() {
                reconcile(&*asker, &channels, &lp).await;
                drop(channels);
                tokio::select! {
                    _ = lp.poke.notified() => {}
                    _ = tokio::time::sleep(EVERY) => {}
                }
            }
        });
    }

    /// A channel changed: look at the outputs now.
    pub(super) fn poke_feeds(&self) {
        if let Some(lp) = self.feed_loop.get() {
            lp.clean.store(false, Ordering::Relaxed);
            lp.poke.notify_one();
        }
    }
}

async fn reconcile(asker: &dyn Asker, channels: &Arc<Channels>, lp: &Loop) {
    let c = channels.clone();
    let Ok(feeds) = tokio::task::spawn_blocking(move || c.programme_feeds()).await else { return };
    if feeds.is_empty() && lp.clean.load(Ordering::Relaxed) {
        return;
    }
    let (first, shows) = (asker.first(), asker.shows());
    let mut notes: Vec<(String, Option<String>)> = Vec::new();
    let mut clean = true;
    for f in &feeds {
        let show = f.show.clone().unwrap_or_else(|| first.clone());
        if !shows.iter().any(|(id, _)| *id == show) {
            let why = format!("there is no show called {show} to take the programme from. Choose the programme again to use the first show, or add the show");
            notes.push((f.channel.clone(), Some(why)));
        }
    }
    for (show, unreachable) in &shows {
        clean &= one_show(asker, show, &first, &feeds, unreachable.as_deref(), lp, &mut notes).await;
    }
    lp.clean.store(clean && feeds.is_empty(), Ordering::Relaxed);
    let c = channels.clone();
    let _ = tokio::task::spawn_blocking(move || notes.into_iter().for_each(|(id, why)| c.note_programme(&id, why))).await;
}

/// Put one show's outputs right. False when it could not be asked, or an
/// output no channel wants would not go.
async fn one_show(asker: &dyn Asker, show: &str, first: &str, feeds: &[ProgrammeFeed], unreachable: Option<&str>, lp: &Loop, notes: &mut Vec<(String, Option<String>)>) -> bool {
    let mine: Vec<&ProgrammeFeed> = feeds.iter().filter(|f| f.show.as_deref().unwrap_or(first) == show).collect();
    let status = match unreachable {
        Some(why) => Err(why.to_string()),
        None => asker.ask(show, "core.status", Value::Null).await.map_err(|e| e.message),
    };
    let status = match status {
        Ok(s) => s,
        Err(why) => {
            let why = format!("show {show} cannot be asked to send its programme here yet ({why}); it is asked again in a few seconds");
            notes.extend(mine.iter().map(|f| (f.channel.clone(), Some(why.clone()))));
            return false;
        }
    };
    let outputs: Vec<String> = status["outputs"].as_array().into_iter().flatten().filter_map(|o| o["id"].as_str().map(String::from)).collect();
    let given: HashMap<String, String> = lp.given.lock().iter().filter(|((s, _), _)| s == show).map(|((_, o), u)| (o.clone(), u.clone())).collect();
    for f in &mine {
        notes.push((f.channel.clone(), None));
    }
    let mut ok = true;
    for step in plan::steps(show, first, feeds, &outputs, &given) {
        let id = step.params["id"].as_str().unwrap_or_default().to_string();
        let answer = asker.ask(show, step.method, step.params.clone()).await;
        match (answer, step.feed) {
            (Ok(_), Some((_, uri))) => {
                lp.given.lock().insert((show.to_string(), id), uri);
            }
            (Ok(_), None) => {
                lp.given.lock().remove(&(show.to_string(), id));
            }
            (Err(e), Some((channel, _))) => {
                notes.retain(|(c, _)| *c != channel);
                notes.push((channel, Some(format!("show {show} would not send its programme here: {}", e.message))));
            }
            (Err(e), None) => {
                ok = false;
                tracing::warn!(%show, output = %id, error = %e.message, "a programme output no channel wants would not go");
            }
        }
    }
    ok
}
