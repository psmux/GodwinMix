//! A channel source's backup rule, kept while the source is gone.
//!
//! The source a channel made for an encoder goes when the encoder stops,
//! unless a scene holds it, and the mixer forgets a removed source's rule.
//! The channel keeps the rule instead, on its record, and the source has it
//! again when the encoder comes back.

use godwinmix_protocol::backup::OnLoss;

use super::Channels;

impl Channels {
    /// Keep the rule `id` had as its source went, on the channel that made
    /// it; `None` drops one kept before.
    pub(super) fn keep_rule(&self, id: &str, rule: Option<OnLoss>) {
        for r in self.records.lock().iter_mut().filter(|r| r.auto_sources.iter().any(|s| s == id)) {
            match &rule {
                Some(rule) => r.auto_rules.insert(id.to_string(), rule.clone()),
                None => r.auto_rules.remove(id),
            };
        }
    }

    /// The rule kept for `id`, while the backup it names is still a source.
    /// A rule naming a source that has since gone would have the show refuse
    /// the stream.
    pub(super) fn kept_rule(&self, channel: &str, id: &str) -> Option<OnLoss> {
        let rule = self.records.lock().iter().find(|r| r.id == channel)?.auto_rules.get(id).cloned()?;
        (self.target().has_source(&rule.backup) == Some(true)).then_some(rule)
    }
}
