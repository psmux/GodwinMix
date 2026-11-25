//! What a guest needs in the mixer, made the first time they offer and taken
//! away with the invite.
//!
//! * the source `guest-<id>` (`guest://<id>`), named after the invite;
//! * the bus `guest-<id>`, a mix minus of that source, which is the sound
//!   of their return: everyone on the programme but themselves;
//! * the output `guest-<id>-return`, a `whep/output` with that bus as its
//!   sound, which the guest plays from `/guest/<id>/whep`.
//!
//! All three go through the mixer's queue the way `source.add`, `audio.bus.add`
//! and `output.add` do, and none of them is saved in the runtime store, so a
//! restart starts with none until the guest comes back.

use super::{return_of, source_of, Guests};
use godwinmix_core::buses::{AudioCall, Refusal};
use godwinmix_core::config::{OutputConfig, SourceConfig};
use godwinmix_core::mixer::Command;
use godwinmix_protocol::buses::BusConfig;
use std::time::Duration;

/// The source a guest's return leaves out may take a moment to be in the
/// mixer after it was added; the bus is asked again this many times.
const BUS_TRIES: usize = 20;

impl Guests {
    /// Make sure the guest's source, bus and return exist. Answers why not.
    pub(super) async fn ensure(&self, id: &str, name: &str) -> Result<(), String> {
        let _one_at_a_time = self.building.lock().await;
        let have = self.mixer.configs().await.map_err(|e| format!("{e:#}"))?;
        let source = source_of(id);
        if !have.sources.iter().any(|s| s.id == source) {
            let mut cfg = SourceConfig::bare(&source, &godwinmix_core::guest::source::uri_of(id));
            cfg.name = Some(name.to_string());
            cfg.muted = id == godwinmix_protocol::guests::TALKBACK;
            self.mixer.request(|ack| Command::AddSource(Box::new(cfg), Some(ack))).await.map_err(|e| format!("{e:#}"))?;
        }
        if id == godwinmix_protocol::guests::TALKBACK {
            self.joined.lock().insert(id.to_string());
            return self.talkback_arrived().await;
        }
        self.ensure_bus(id).await?;
        let output = return_of(id);
        if !have.outputs.iter().any(|o| o.id == output) {
            let cfg = return_output(id);
            self.mixer.request(|ack| Command::AddOutput(Box::new(cfg), Some(ack))).await.map_err(|e| format!("{e:#}"))?;
        }
        self.joined.lock().insert(id.to_string());
        Ok(())
    }

    async fn ensure_bus(&self, id: &str) -> Result<(), String> {
        let mut cfg = BusConfig::named(&source_of(id));
        cfg.name = Some(format!("Return for {id}"));
        cfg.minus = Some(source_of(id));
        for _ in 0..BUS_TRIES {
            match self.mixer.audio(AudioCall::AddGuest(cfg.clone())).await.map_err(|e| format!("{e:#}"))? {
                Ok(_) => return Ok(()),
                Err(Refusal::Invalid { message, .. }) if message.contains("already a bus") => return Ok(()),
                Err(Refusal::NotFound { kind: "source", .. }) => tokio::time::sleep(Duration::from_millis(100)).await,
                Err(other) => return Err(other.to_string()),
            }
        }
        Err(format!("the guest's source {} did not reach the mixer, so its return could not be made", source_of(id)))
    }

    /// Take the guest's return, bus and source away, in that order: the
    /// output reads the bus and the bus reads the source.
    pub(super) async fn leave(&self, id: &str) {
        let _one_at_a_time = self.building.lock().await;
        self.joined.lock().remove(id);
        let output = return_of(id);
        if let Err(e) = self.mixer.request(|ack| Command::RemoveOutput(output.clone(), Some(ack))).await {
            tracing::debug!(%output, error = %format!("{e:#}"), "a guest's return was not there to remove");
        }
        if let Ok(Err(e)) = self.mixer.audio(AudioCall::Remove(source_of(id))).await {
            tracing::debug!(guest = %id, error = %e, "a guest's bus was not there to remove");
        }
        let source = source_of(id);
        if let Err(e) = self.mixer.request(|ack| Command::RemoveSource(source.clone(), Some(ack))).await {
            tracing::debug!(%source, error = %format!("{e:#}"), "a guest's source was not there to remove");
        }
    }
}

/// The `whep/output` a guest's return is: the programme picture as it is
/// encoded, and the guest's mix minus as its sound.
fn return_output(id: &str) -> OutputConfig {
    let mut cfg = OutputConfig::bare(&return_of(id), "");
    cfg.type_id = Some("whep/output".into());
    cfg.audio = Some(source_of(id));
    cfg.params.insert(godwinmix_core::guest::RETURN_PARAM.into(), toml::Value::String(id.to_string()));
    // One viewer: the guest. A second tab of theirs takes its place.
    cfg.params.insert("max_viewers".into(), toml::Value::Integer(2));
    cfg
}
