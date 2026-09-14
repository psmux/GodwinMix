//! The programme encoder, started by the first consumer and stopped after the
//! last one leaves.
//!
//! # Why this module exists
//!
//! The encode chain used to be built in `Mixer::build`, linked to the raw
//! programme tees, and taken to PLAYING with the rest of the pipeline. A core
//! with no output attached therefore encoded a black slate from boot for
//! nobody. On an M4 Pro with VideoToolbox that measured 0.022 of a core; on a
//! Pi 5, where the encoder is software x264, it is the largest single cost an
//! idle mixer pays.
//!
//! The raw programme never stops. The compositor, the audio mixer, the level
//! meter and both raw tees stay in PLAYING whatever this module does, so the
//! picture is always there and a take is never waiting for anything. Only the
//! encode chain (queue, conversion, encoder, parser, encoded tee) comes and
//! goes.
//!
//! # How it comes and goes
//!
//! A consumer is anything that reads encoded programme data: an output, a WHEP
//! session, a recording. Each takes a [`Lease`] from the [`EncoderHandle`] and
//! holds it for as long as it wants the encoder. The first lease asks the
//! mixer thread to attach; the last one to drop asks it to detach. As with the
//! mosaic, the asking is a message on the mixer's own queue, so the state
//! changes still happen on the one thread that owns them.
//!
//! Attaching is: request a pad on the raw tee, link it to the head of the
//! chain, bring the chain up to the pipeline's state, force a keyframe.
//! Detaching is the reverse, and it unlinks before it changes state so no
//! buffer is ever pushed into an element on its way to NULL.
//!
//! The tees carry `allow-not-linked`, so a detached chain is simply a tee with
//! one fewer pad. Nothing upstream notices. This is the same runtime tee
//! surgery `OutputSlot::attach` and `OutputSlot::detach` have always done.
//!
//! # The keyframe
//!
//! An output that arrives at an idle core is the reason the encoder starts, so
//! it must not then wait a GOP for a picture. `attach` sends an upstream
//! force-key-unit event as its last act, which makes the first frame out of a
//! freshly started encoder an IDR.

use crate::gstutil;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// `[program] encoder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncoderPolicy {
    /// Built and running from boot, whether or not anything reads it. What
    /// every release before this one did, kept for a mixer that would rather
    /// spend the CPU than think about it.
    Always,
    /// Started by the first consumer, stopped after the last leaves.
    #[default]
    OnDemand,
}

impl EncoderPolicy {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "always" => Some(Self::Always),
            "on-demand" | "on_demand" | "ondemand" => Some(Self::OnDemand),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::OnDemand => "on-demand",
        }
    }
}

/// What the handle asks the mixer thread to do, carrying the generation it was
/// decided at so a command overtaken by a newer one is dropped rather than
/// applied out of order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderDemand {
    Attach { generation: u64 },
    Detach { generation: u64 },
}

impl EncoderDemand {
    pub fn generation(self) -> u64 {
        match self {
            Self::Attach { generation } | Self::Detach { generation } => generation,
        }
    }
}

type DemandSink = Arc<dyn Fn(EncoderDemand) + Send + Sync>;

/// What `/metrics` and `core.status` read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EncoderStats {
    pub policy: &'static str,
    /// Whether the encode chain is linked and running right now.
    pub running: bool,
    /// Live leases, by kind: `output`, `whep`, `record`.
    pub consumers: BTreeMap<String, u64>,
    pub total: u64,
    /// Times the chain has been started since boot.
    pub starts: u64,
}

struct Shared {
    policy: EncoderPolicy,
    /// Lease kind to count. A map rather than a number so `/metrics` can say
    /// which kind of client is keeping the encoder up.
    holders: Mutex<BTreeMap<String, u64>>,
    next_id: AtomicU64,
    /// Bumped by every acquire and every release. A demand carries the value
    /// it was decided at, and the mixer thread refuses one that is not the
    /// current value, so a lease taken between the decision and the delivery
    /// cannot be lost.
    generation: AtomicU64,
    running: AtomicBool,
    starts: AtomicU64,
    demand: Option<DemandSink>,
}

impl Shared {
    fn total(&self) -> u64 {
        self.holders.lock().values().sum()
    }

    fn ask(&self, d: EncoderDemand) {
        if let Some(sink) = &self.demand {
            sink(d);
        }
    }

    /// Work out what the current lease count implies and say so, carrying the
    /// generation the decision was made at.
    fn settle(&self) {
        if self.policy == EncoderPolicy::Always {
            return;
        }
        // The count and the generation are read under the same lock, so the
        // number in the message and the number it was decided from agree.
        let holders = self.holders.lock();
        let total: u64 = holders.values().sum();
        let generation = self.generation.load(Ordering::SeqCst);
        drop(holders);
        if total > 0 {
            self.ask(EncoderDemand::Attach { generation });
        } else {
            self.ask(EncoderDemand::Detach { generation });
        }
    }
}

/// The public face of the encoder's lifecycle. Cloneable and cheap.
#[derive(Clone)]
pub struct EncoderHandle {
    shared: Arc<Shared>,
}

impl EncoderHandle {
    pub fn new(policy: EncoderPolicy, demand: DemandSink) -> Self {
        Self::with_sink(policy, Some(demand))
    }

    /// A handle attached to nothing: it counts leases and answers questions,
    /// but no chain is ever asked for. For tests and for `gmx bench`.
    pub fn detached(policy: EncoderPolicy) -> Self {
        Self::with_sink(policy, None)
    }

    fn with_sink(policy: EncoderPolicy, demand: Option<DemandSink>) -> Self {
        Self {
            shared: Arc::new(Shared {
                policy,
                holders: Mutex::new(BTreeMap::new()),
                next_id: AtomicU64::new(1),
                generation: AtomicU64::new(0),
                running: AtomicBool::new(false),
                starts: AtomicU64::new(0),
                demand,
            }),
        }
    }

    pub fn policy(&self) -> EncoderPolicy {
        self.shared.policy
    }

    /// Ask for the encoder and keep it up for as long as the guard lives.
    ///
    /// `kind` is what shows in `gmx_encoder_consumers{kind}`: `output`,
    /// `whep`, `record`.
    pub fn lease(&self, kind: &str) -> Lease {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        {
            let mut holders = self.shared.holders.lock();
            *holders.entry(kind.to_string()).or_insert(0) += 1;
        }
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        self.shared.settle();
        Lease { shared: self.shared.clone(), kind: kind.to_string(), id }
    }

    /// Whether the encode chain is linked and running.
    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    pub fn consumers(&self) -> u64 {
        self.shared.total()
    }

    pub fn stats(&self) -> EncoderStats {
        let holders = self.shared.holders.lock().clone();
        EncoderStats {
            policy: self.shared.policy.as_str(),
            running: self.is_running(),
            total: holders.values().sum(),
            consumers: holders,
            starts: self.shared.starts.load(Ordering::Relaxed),
        }
    }

    /// Called by the mixer once the chain has actually been attached or
    /// detached.
    pub fn mark_running(&self, running: bool) {
        let was = self.shared.running.swap(running, Ordering::AcqRel);
        if running && !was {
            self.shared.starts.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Whether a demand that arrived on the mixer thread is still the truth.
    ///
    /// The demand carries the generation it was decided at. A lease taken or
    /// dropped since then has bumped the generation, and its own `settle` is
    /// already on the queue behind this one, so the stale command is refused
    /// and the fresh one decides. Without this a subscriber that arrived
    /// between the emptiness check and the delivery would see a detach applied
    /// after its own attach.
    pub fn accepts(&self, demand: EncoderDemand) -> bool {
        self.shared.generation.load(Ordering::SeqCst) == demand.generation()
    }

    /// What the current leases add up to, read on the mixer thread so a
    /// command that is accepted is applied against the count as it is now.
    pub fn wanted(&self) -> bool {
        self.shared.policy == EncoderPolicy::Always || self.shared.total() > 0
    }
}

/// Proof that somebody is reading the encoded programme. Dropping it lets the
/// encoder stop.
pub struct Lease {
    shared: Arc<Shared>,
    kind: String,
    #[allow(dead_code)]
    id: u64,
}

impl Lease {
    pub fn kind(&self) -> &str {
        &self.kind
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        {
            let mut holders = self.shared.holders.lock();
            if let Some(n) = holders.get_mut(&self.kind) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    holders.remove(&self.kind);
                }
            }
        }
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        self.shared.settle();
    }
}

/// The encode chain itself: the elements, the tee they hang off, and the pad
/// that joins them while it is running.
///
/// One of these per medium. The mixer owns two, video and audio, and moves
/// them together.
pub struct EncodeChain {
    tag: &'static str,
    tee: gst::Element,
    /// Head first. The head's sink pad is what the tee links to.
    chain: Vec<gst::Element>,
    pad: Option<gst::Pad>,
}

impl EncodeChain {
    /// `chain` must already be added to the pipeline and linked together.
    ///
    /// A chain that is already joined to the tee is adopted in that state: the
    /// tee pad it is hanging off is found from the head's peer, so the caller
    /// does not have to hand it over and `Mixer::build` keeps the linking it
    /// always had. The only thing this owns is whether the join stays.
    pub fn new(tag: &'static str, tee: &gst::Element, chain: Vec<gst::Element>) -> Self {
        let pad = chain
            .first()
            .and_then(|head| head.static_pad("sink"))
            .and_then(|sink| sink.peer())
            .filter(|peer| peer.parent().as_ref() == Some(tee.upcast_ref()));
        Self { tag, tee: tee.clone(), chain, pad }
    }

    pub fn is_attached(&self) -> bool {
        self.pad.is_some()
    }

    /// The state the first element of the chain is in, for a test that wants
    /// to prove an idle encoder is not merely flagged off.
    pub fn state(&self) -> gst::State {
        self.chain
            .first()
            .map(|e| e.state(gst::ClockTime::ZERO).1)
            .unwrap_or(gst::State::Null)
    }

    /// The encoder element's state, by name, for the same reason.
    pub fn element_state(&self, name: &str) -> Option<gst::State> {
        self.chain
            .iter()
            .find(|e| e.name() == name)
            .map(|e| e.state(gst::ClockTime::ZERO).1)
    }

    /// Bring the chain up and join it to the tee.
    ///
    /// The lock on each element's state is lifted first, then the chain is
    /// linked, then it is brought up to whatever the pipeline is doing. The
    /// last act is a force-key-unit event upstream, so the first frame out of
    /// a freshly started encoder is an IDR and an output that arrived at an
    /// idle core is not waiting a GOP for a picture.
    pub fn attach(&mut self) -> Result<()> {
        if self.pad.is_some() {
            return Ok(());
        }
        let head = self
            .chain
            .first()
            .context("an encode chain with no elements cannot be attached")?;
        let sink = head.static_pad("sink").context("the head of the chain has no sink pad")?;
        for el in &self.chain {
            el.set_locked_state(false);
        }
        let pad = self
            .tee
            .request_pad_simple("src_%u")
            .context("the raw programme tee refused a pad for the encoder")?;
        pad.link(&sink).context("linking the encoder onto the raw programme tee")?;
        // Tail first, so every element downstream of the encoder is ready to
        // take a buffer before the encoder can produce one.
        for el in self.chain.iter().rev() {
            el.sync_state_with_parent()
                .with_context(|| format!("{} encode chain would not start", self.tag))?;
        }
        gstutil::force_keyframe(&pad);
        self.pad = Some(pad);
        debug!(chain = self.tag, "encode chain attached");
        Ok(())
    }

    /// Unlink from the tee and take the chain down.
    ///
    /// Unlinking comes first: an element on its way to NULL must never be
    /// handed another buffer. The tee carries `allow-not-linked`, so losing a
    /// branch is nothing to it.
    ///
    /// Each element's state is locked afterwards. Without that the next
    /// `pipeline.set_state(Playing)`, which is what `Mixer::start` does after
    /// `Mixer::build` has armed this, would walk the bin and take the whole
    /// chain back up: a locked element is skipped by its parent's state
    /// changes and stays where it was put.
    pub fn detach(&mut self) {
        let Some(pad) = self.pad.take() else {
            // Never attached, but still lock it down so the pipeline's own
            // state changes leave it alone.
            for el in &self.chain {
                el.set_locked_state(true);
                let _ = el.set_state(gst::State::Null);
            }
            return;
        };
        if let Some(peer) = pad.peer() {
            if let Err(e) = pad.unlink(&peer) {
                warn!(chain = self.tag, ?e, "could not unlink the encode chain");
            }
        }
        self.tee.release_request_pad(&pad);
        for el in self.chain.iter().rev() {
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
        }
        debug!(chain = self.tag, "encode chain detached");
    }
}

/// Both chains, moved together, with the handle they report to.
///
/// The mixer holds one of these and passes every `Command::Encoder` to
/// [`Encoder::demand`]. Nothing else in `mixer.rs` has to know how the chain
/// is wired.
pub struct Encoder {
    handle: EncoderHandle,
    video: EncodeChain,
    audio: EncodeChain,
}

impl Encoder {
    pub fn new(handle: EncoderHandle, video: EncodeChain, audio: EncodeChain) -> Self {
        Self { handle, video, audio }
    }

    pub fn handle(&self) -> EncoderHandle {
        self.handle.clone()
    }

    pub fn is_running(&self) -> bool {
        self.video.is_attached()
    }

    /// The states a test reads: the video encoder element and the audio one.
    pub fn element_states(&self, video: &str, audio: &str) -> (Option<gst::State>, Option<gst::State>) {
        (self.video.element_state(video), self.audio.element_state(audio))
    }

    /// What `Mixer::build` does once, before the pipeline starts: with
    /// `on-demand` the chains come apart so that a core that is asked for
    /// nothing encodes nothing.
    pub fn arm(&mut self) -> Result<()> {
        match self.handle.policy() {
            EncoderPolicy::Always => {
                self.video.attach()?;
                self.audio.attach()?;
                self.handle.mark_running(true);
                info!(policy = "always", "programme encoder runs from boot");
            }
            EncoderPolicy::OnDemand => {
                self.video.detach();
                self.audio.detach();
                self.handle.mark_running(false);
                info!(
                    policy = "on-demand",
                    "programme encoder waits for its first consumer"
                );
            }
        }
        Ok(())
    }

    /// One demand off the mixer's queue.
    ///
    /// A command decided at a generation that is no longer current is refused
    /// here and the count is reconciled instead, which is what stops a
    /// consumer that arrived between the decision and the delivery from losing
    /// the encoder it just asked for.
    pub fn demand(&mut self, d: EncoderDemand) -> Result<()> {
        if self.handle.policy() == EncoderPolicy::Always {
            return Ok(());
        }
        if !self.handle.accepts(d) {
            debug!(?d, "stale encoder demand refused, a newer one is behind it");
            return Ok(());
        }
        // Decided from the count as it stands on this thread, not from the
        // verb in the message.
        let wanted = self.handle.wanted();
        self.reconcile(wanted)
    }

    /// Make the chain match `wanted`, doing nothing when it already does.
    pub fn reconcile(&mut self, wanted: bool) -> Result<()> {
        if wanted == self.video.is_attached() {
            return Ok(());
        }
        if wanted {
            self.video.attach()?;
            self.audio.attach()?;
            self.handle.mark_running(true);
            info!("programme encoder started for its first consumer");
        } else {
            self.video.detach();
            self.audio.detach();
            self.handle.mark_running(false);
            info!("programme encoder stopped, nothing is reading it");
        }
        Ok(())
    }

    /// Take everything down, for shutdown.
    pub fn shutdown(&mut self) {
        self.video.detach();
        self.audio.detach();
        self.handle.mark_running(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_policy_reads_both_spellings_and_nothing_else() {
        assert_eq!(EncoderPolicy::parse("always"), Some(EncoderPolicy::Always));
        assert_eq!(EncoderPolicy::parse("on-demand"), Some(EncoderPolicy::OnDemand));
        assert_eq!(EncoderPolicy::parse("on_demand"), Some(EncoderPolicy::OnDemand));
        assert_eq!(EncoderPolicy::parse("sometimes"), None);
        assert_eq!(EncoderPolicy::default(), EncoderPolicy::OnDemand);
    }

    #[test]
    fn the_first_lease_asks_for_an_attach_and_the_last_drop_asks_for_a_detach() {
        let seen: Arc<Mutex<Vec<EncoderDemand>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = {
            let seen = seen.clone();
            Arc::new(move |d: EncoderDemand| seen.lock().push(d)) as DemandSink
        };
        let h = EncoderHandle::new(EncoderPolicy::OnDemand, sink);
        assert_eq!(h.consumers(), 0);
        assert!(!h.wanted());

        let a = h.lease("output");
        assert!(matches!(seen.lock()[0], EncoderDemand::Attach { .. }));
        assert!(h.wanted());
        let b = h.lease("whep");
        assert_eq!(h.consumers(), 2);
        assert_eq!(h.stats().consumers.get("whep").copied(), Some(1));

        drop(a);
        // One consumer left, so the settle asks to attach again rather than
        // to stop.
        assert!(matches!(seen.lock().last().unwrap(), EncoderDemand::Attach { .. }));
        drop(b);
        assert!(matches!(seen.lock().last().unwrap(), EncoderDemand::Detach { .. }));
        assert!(!h.wanted());
        assert_eq!(h.stats().consumers.len(), 0);
    }

    /// The finding this generation exists for: a consumer arriving between a
    /// detach being decided and it reaching the mixer thread must not lose the
    /// encoder.
    #[test]
    fn a_lease_taken_after_a_detach_was_decided_refuses_it() {
        let h = EncoderHandle::detached(EncoderPolicy::OnDemand);
        let a = h.lease("output");
        // What the drop would put on the queue.
        drop(a);
        let stale = EncoderDemand::Detach { generation: 2 };
        assert!(h.accepts(stale), "the generation right after the drop is current");
        // A new consumer arrives before the mixer thread gets to it.
        let _b = h.lease("whep");
        assert!(!h.accepts(stale), "a stale detach must be refused");
        assert!(h.wanted());
    }

    /// A mixer small enough to start inside a test.
    fn mixer_cfg(policy: &str) -> crate::config::Config {
        let mut cfg: crate::config::Config = toml::from_str("").unwrap();
        cfg.canvas = crate::config::Canvas {
            width: 320,
            height: 180,
            fps: 15,
            sample_rate: 48000,
            channels: 2,
        };
        cfg.multiview.enabled = false;
        cfg.program.encoder = policy.to_string();
        cfg
    }

    /// The acceptance test for the switch: nothing asked for, nothing
    /// encoding, and the encoder element really is at NULL rather than merely
    /// flagged off.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_idle_core_on_demand_has_no_encoder_running() {
        let _ = gst::init();
        let (mut mix, handle, cmd_rx, _bus_rx) =
            crate::mixer::Mixer::build(mixer_cfg("on-demand")).unwrap();
        mix.start().unwrap();
        assert!(!mix.encoder_running(), "an idle core is encoding and must not be");
        let (venc, aenc) = mix.encoder_element_states();
        assert_eq!(venc, Some(gst::State::Null), "the video encoder is not at NULL");
        assert_eq!(aenc, Some(gst::State::Null), "the audio encoder is not at NULL");
        let enc = mix.encoder_handle();
        assert_eq!(enc.consumers(), 0);
        assert!(!enc.is_running());
        assert_eq!(enc.stats().policy, "on-demand");

        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        assert!(!enc.is_running(), "the encoder started with nobody reading it");
        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    /// And the other half: a consumer starts it, and it stops again when the
    /// consumer goes.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_consumer_starts_the_encoder_and_leaving_stops_it() {
        let _ = gst::init();
        let (mut mix, handle, cmd_rx, _bus_rx) =
            crate::mixer::Mixer::build(mixer_cfg("on-demand")).unwrap();
        mix.start().unwrap();
        let enc = mix.encoder_handle();
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());

        let lease = enc.lease("whep");
        for _ in 0..40 {
            if enc.is_running() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(enc.is_running(), "a consumer asked and the encoder never started");
        assert_eq!(enc.stats().starts, 1);

        drop(lease);
        for _ in 0..40 {
            if !enc.is_running() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(!enc.is_running(), "the encoder outlived its last consumer");
        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    /// `always` is the old behaviour, kept working.
    #[tokio::test(flavor = "multi_thread")]
    async fn always_encodes_from_boot() {
        let _ = gst::init();
        let (mut mix, handle, cmd_rx, _bus_rx) =
            crate::mixer::Mixer::build(mixer_cfg("always")).unwrap();
        mix.start().unwrap();
        assert!(mix.encoder_running(), "always must encode from boot");
        let enc = mix.encoder_handle();
        assert!(enc.is_running());
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());
        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }

    #[test]
    fn always_never_asks_for_anything() {
        let seen: Arc<Mutex<Vec<EncoderDemand>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = {
            let seen = seen.clone();
            Arc::new(move |d: EncoderDemand| seen.lock().push(d)) as DemandSink
        };
        let h = EncoderHandle::new(EncoderPolicy::Always, sink);
        let a = h.lease("output");
        drop(a);
        assert!(seen.lock().is_empty(), "an always-on encoder must ask for nothing");
        assert!(h.wanted(), "and it is always wanted");
    }
}
