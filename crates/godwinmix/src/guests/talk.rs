//! Talkback: the operator speaking to one guest, or to all of them, off air.
//!
//! The source that talks is `guest-talkback`, the operator's browser
//! microphone published to `/guest/talkback/whip` with an operate token, or
//! another input chosen with `source`. Whichever it is, the desk keeps it
//! muted on the programme for as long as it is the talkback source, and adds
//! it to a guest's return bus only while talkback is held there. Holding is
//! not saved: a restart starts with nobody being talked to.

use super::{source_of, Guests};
use godwinmix_core::buses::{AudioCall, Refusal, TalkCall};
use godwinmix_core::guest::Link;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::guests::{GuestList, TalkbackRequest, TalkbackState, TALKBACK};
use std::collections::BTreeSet;

pub struct Talk {
    /// The source that talks.
    pub source: String,
    /// The guests it is held into.
    pub held: BTreeSet<String>,
}

impl Default for Talk {
    fn default() -> Talk {
        Talk { source: source_of(TALKBACK), held: BTreeSet::new() }
    }
}

impl Guests {
    pub(super) fn talk_state(&self) -> TalkbackState {
        let t = self.talk.lock();
        let present = t.source != source_of(TALKBACK) || godwinmix_core::guest::link(TALKBACK) == Link::Live;
        TalkbackState { source: t.source.clone(), present, held: t.held.iter().cloned().collect() }
    }

    /// The operator's microphone has just been added: keep it off the programme.
    pub(super) async fn talkback_arrived(&self) -> Result<(), String> {
        // The source was muted when it was added; this keeps it so. It may
        // take a moment to be in the mixer after the add was answered.
        for _ in 0..20 {
            let source = source_of(TALKBACK);
            match self.mixer.audio(AudioCall::Talker { source, on: true }).await.map_err(|e| format!("{e:#}"))? {
                Ok(_) => return Ok(()),
                Err(Refusal::NotFound { .. }) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
                Err(other) => return Err(other.to_string()),
            }
        }
        Err("the talkback microphone did not reach the mixer".into())
    }

    pub async fn set_talkback(&self, req: TalkbackRequest) -> Result<GuestList, RpcError> {
        if let Some(source) = req.source.as_deref().filter(|s| *s != self.talk.lock().source) {
            self.choose_source(source).await?;
        }
        let targets: Vec<String> = match &req.guest {
            Some(id) if !self.invites.lock().contains_key(id) => return Err(self.no_invite(id)),
            Some(id) if !self.joined.lock().contains(id) => {
                let msg = format!("{id} has not joined yet, so there is no return to talk into. Send them the link; talkback works once they are live.");
                return Err(RpcError::not_in_state(msg).with("guest", id.as_str()).with("state", "invited"));
            }
            Some(id) => vec![id.clone()],
            None => self.joined.lock().iter().filter(|id| *id != TALKBACK).cloned().collect(),
        };
        let source = self.talk.lock().source.clone();
        let buses = targets.iter().map(|id| source_of(id)).collect();
        let call = AudioCall::Talk(TalkCall { source: source.clone(), buses, on: req.on });
        let answer = self.mixer.audio(call).await.map_err(|e| RpcError::internal(format!("{e:#}")))?;
        answer.map_err(|r| no_talker(r, &source))?;
        {
            let mut t = self.talk.lock();
            for id in &targets {
                if req.on {
                    t.held.insert(id.clone());
                } else {
                    t.held.remove(id);
                }
            }
        }
        self.changed();
        Ok(self.list())
    }

    /// Make `source` the one that talks, and the one before it an ordinary
    /// source again. The old one stays muted: unmuting it is the operator's
    /// call, not this one's.
    async fn choose_source(&self, source: &str) -> Result<(), RpcError> {
        let old = self.talk.lock().source.clone();
        let mark = |source: &str, on| AudioCall::Talker { source: source.to_string(), on };
        let _ = self.mixer.audio(mark(&old, false)).await;
        let _ = self.mixer.audio(AudioCall::Talk(TalkCall { source: old, buses: self.buses(), on: false })).await;
        self.mixer.audio(mark(source, true)).await.map_err(|e| RpcError::internal(format!("{e:#}")))?.map_err(|r| no_talker(r, source))?;
        let mut t = self.talk.lock();
        t.source = source.to_string();
        t.held.clear();
        Ok(())
    }

    fn buses(&self) -> Vec<String> {
        self.joined.lock().iter().filter(|id| *id != TALKBACK).map(|id| source_of(id)).collect()
    }
}

fn no_talker(r: Refusal, source: &str) -> RpcError {
    match r {
        Refusal::NotFound { kind: "source", known, .. } => {
            let msg = format!(
                "the talkback source {source} is not in the mixer. Press Talk in the guests panel to send this browser's \
                 microphone, or pass source with an input that is there."
            );
            RpcError::new(ErrorCode::NotFound, msg).with("kind", "source").with("id", source).with("valid", known)
        }
        other => RpcError::not_in_state(other.to_string()),
    }
}
