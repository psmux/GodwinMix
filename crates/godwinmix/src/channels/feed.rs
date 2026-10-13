//! What feeds a channel its picture, kept with the channel.
//!
//! * `programme`: a show publishes its programme into the channel over RTMP
//!   on loopback, with a key of the channel's own made for it. The show's
//!   output is run by whoever owns the shows (`station/channel_feed.rs`, or
//!   `control/methods/channel_feed.rs` in a single process core) from
//!   [`Channels::programme_feeds`].
//! * `encoder`: somebody publishes to the channel's address. Nothing to run.
//! * `pull`: the listener pulls the address and publishes it into the
//!   channel itself. The address is sealed under `channel.<id>.pull`, like
//!   a destination's, and handed to the listener in the table.

use std::time::{Duration, Instant};

use godwinmix_protocol::channel_feed::{check_pull_url, ChannelFeed, FeedKind, FeedRequest, FeedState};
use godwinmix_protocol::channel_ingest::ChannelProtocol;
use godwinmix_protocol::channels::KeyMode;
use godwinmix_protocol::destination::uri_host;
use godwinmix_protocol::error::RpcError;
use serde::{Deserialize, Serialize};

use super::{keys, Channels, Record};

/// What a programme output's id starts with; the rest is the channel's id.
pub const OUTPUT_PREFIX: &str = "to-channel-";
/// The label of the key the programme publishes with.
const KEY_LABEL: &str = "Programme";

/// A feed as the channels file keeps it. An encoder is the default, so a
/// channel from before feeds reads as one and is written as before.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum StoredFeed {
    #[default]
    Encoder,
    Programme {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<String>,
        /// The id of the channel key the programme publishes with.
        key: String,
    },
    Pull {
        uri_host: String,
    },
}

impl StoredFeed {
    pub fn is_encoder(&self) -> bool {
        *self == StoredFeed::Encoder
    }
}

/// What a feed last said: a pull's state from the listener, or why the
/// programme could not be sent.
#[derive(Debug, Clone)]
pub struct Seen {
    channel: String,
    state: FeedState,
    error: Option<String>,
    retry_at: Option<Instant>,
}

/// One channel the programme is to be sent into.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgrammeFeed {
    pub channel: String,
    /// The show named, or `None` for the first one.
    pub show: Option<String>,
    pub output: String,
    pub uri: String,
}

pub fn output_id(channel: &str) -> String {
    format!("{OUTPUT_PREFIX}{channel}")
}

fn pull_scope(channel: &str) -> String {
    format!("{}.pull", keys::scope(channel))
}

impl Channels {
    /// Give a channel a new feed: seal a pull's address, make the key the
    /// programme publishes with, and take back what the old feed had. The
    /// caller commits.
    pub(super) fn take_feed(&self, id: &str, req: FeedRequest) -> Result<(), RpcError> {
        let record = self.records.lock().iter().find(|r| r.id == id).cloned();
        let record = record.ok_or_else(|| self.not_found(id))?;
        let feed = match req {
            FeedRequest::Encoder => StoredFeed::Encoder,
            FeedRequest::Pull { url } => {
                let url = check_pull_url(&url).map_err(|why| RpcError::invalid_params(format!("{why}.")).with("field", "feed.url"))?;
                self.secrets.set(&pull_scope(id), "url", &url).map_err(|e| RpcError::internal(format!("sealing the pull address: {e:#}")))?;
                StoredFeed::Pull { uri_host: uri_host(&url) }
            }
            FeedRequest::Programme { show } => {
                if !record.protocols.contains(&ChannelProtocol::Rtmp) {
                    return Err(RpcError::invalid_params(format!(
                        "the programme reaches channel {id} over RTMP, which it has switched off. Switch RTMP on for it \
                         (channel.set with protocols including \"rtmp\"), then choose the programme again."
                    ))
                    .with("field", "feed")
                    .with("channel", id));
                }
                let show = show.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                let key = match &record.feed {
                    StoredFeed::Programme { key, .. } if record.keys.iter().any(|k| &k.id == key) => key.clone(),
                    _ => self.make_key(id, Some(KEY_LABEL.into()), None)?.id,
                };
                StoredFeed::Programme { show, key }
            }
        };
        self.drop_old_feed(&record, &feed);
        if let Some(r) = self.records.lock().iter_mut().find(|r| r.id == id) {
            r.feed = feed;
        }
        self.feeds.lock().retain(|s| s.channel != id);
        Ok(())
    }

    /// Forget the old feed's sealed address, and the key it published with.
    fn drop_old_feed(&self, record: &Record, new: &StoredFeed) {
        if matches!(record.feed, StoredFeed::Pull { .. }) && !matches!(new, StoredFeed::Pull { .. }) {
            self.secrets.forget(&pull_scope(&record.id));
        }
        if let StoredFeed::Programme { key, .. } = &record.feed {
            if !matches!(new, StoredFeed::Programme { .. }) {
                let _ = self.secrets.set(&keys::scope(&record.id), key, "");
                if let Some(r) = self.records.lock().iter_mut().find(|r| r.id == record.id) {
                    r.keys.retain(|k| &k.id != key);
                }
            }
        }
    }

    /// A pull's whole address, for the listener's table.
    pub(super) fn pull_url(&self, r: &Record) -> Option<String> {
        matches!(r.feed, StoredFeed::Pull { .. }).then(|| self.secrets.get(&pull_scope(&r.id), "url")).flatten()
    }

    /// Seal a pull's address that came in a project file.
    pub(super) fn seal_pull(&self, id: &str, url: &str) -> Result<(), RpcError> {
        let url = check_pull_url(url).map_err(|why| RpcError::invalid_params(format!("channel {id} in the file pulls from an address this mixer refuses: {why}.")))?;
        self.secrets.set(&pull_scope(id), "url", &url).map_err(|e| RpcError::internal(format!("sealing the pull address: {e:#}")))
    }

    /// Whether this mixer holds a pull address for the channel.
    pub(super) fn has_pull(&self, id: &str) -> bool {
        self.secrets.get(&pull_scope(id), "url").is_some_and(|u| !u.is_empty())
    }

    /// A channel fed by the programme whose key did not come with it gets a
    /// new one, or the programme would never reach it.
    pub(super) fn programme_key_for(&self, id: &str) -> Result<String, RpcError> {
        Ok(self.make_key(id, Some(KEY_LABEL.into()), None)?.id)
    }

    /// Forget a removed channel's pull address.
    pub(super) fn forget_feed(&self, id: &str) {
        self.secrets.forget(&pull_scope(id));
        self.feeds.lock().retain(|s| s.channel != id);
    }
}

#[path = "feed_run.rs"]
mod run;

#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;
