//! Clocks across a network.
//!
//! Everything in the core runs on the programme clock, and a running time only
//! means something if both machines agree what it is. So the core publishes
//! its programme clock with a `GstNetTimeProvider`, every node slaves a
//! `GstNetClientClock` to it and waits for `synced` before it starts a single
//! plugin, and PTP is a config switch for a wired LAN where the microseconds
//! matter.
//!
//! The number that says whether any of it is working is the offset: how far
//! the node's idea of now sits from the core's. It goes on every heartbeat,
//! into `gmx_node_clock_offset_ms`, and into `pipeline.clock`.

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_net as gst_net;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

/// The port the core offers its clock on unless the config says otherwise.
/// Above the control port and below the media range, so a firewall rule reads
/// sensibly.
pub const DEFAULT_CLOCK_PORT: i32 = 8447;

/// How long a node waits for its clock to sync before it gives up and says so.
/// A LAN settles in under a second; thirty is the point at which something is
/// wrong rather than slow.
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(30);

/// The core's side: a time provider sitting on the programme clock.
pub struct Provider {
    _provider: gst_net::NetTimeProvider,
    port: i32,
}

impl Provider {
    /// Publish `clock` on `port`. `port` may be 0, in which case the kernel
    /// picks and `port()` says which.
    ///
    /// Binding to every address on purpose: a node arrives on whichever
    /// interface the operator wired, and the core does not know which that is
    /// until it does.
    pub fn publish(clock: &gst::Clock, port: i32) -> Result<Self> {
        let provider = gst_net::NetTimeProvider::new(clock, None, port)
            .context("start the net time provider on the programme clock")?;
        let bound = provider.property::<i32>("port");
        tracing::info!(port = bound, "the programme clock is on the network for nodes");
        Ok(Self { _provider: provider, port: bound })
    }

    pub fn port(&self) -> i32 {
        self.port
    }
}

/// Which kind of clock a node is following.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `GstNetClientClock` against the core's provider. Works on any network.
    Net,
    /// `GstPtpClock`. Wired LANs, hardware timestamping, microseconds.
    Ptp,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "net" | "" => Some(Kind::Net),
            "ptp" => Some(Kind::Ptp),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Net => "net",
            Kind::Ptp => "ptp",
        }
    }
}

/// The node's side: a clock slaved to the core's, and the readings that say
/// how well.
pub struct Follower {
    clock: gst::Clock,
    kind: Kind,
    samples: Mutex<Vec<f64>>,
}

impl Follower {
    /// Follow the core's clock. Returns as soon as the clock object exists;
    /// `wait_for_sync` is the part that blocks.
    ///
    /// PTP is asked for and not insisted on: a machine whose network card or
    /// permissions will not do PTP falls back to the net clock with a warning
    /// rather than refusing to start, because a node that will not come up is
    /// worse than one running a millisecond out.
    pub fn follow(host: &str, port: i32, kind: Kind) -> Result<Arc<Self>> {
        let clock = match kind {
            Kind::Ptp => match ptp_clock() {
                Ok(clock) => clock,
                Err(e) => {
                    tracing::warn!(
                        ?e,
                        "PTP was asked for and this machine will not do it; falling back to the \
                         core's net clock. Set clock = \"net\" on this node to stop asking"
                    );
                    net_clock(host, port)?
                }
            },
            Kind::Net => net_clock(host, port)?,
        };
        Ok(Arc::new(Self { clock, kind, samples: Mutex::new(Vec::new()) }))
    }

    pub fn clock(&self) -> &gst::Clock {
        &self.clock
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn synced(&self) -> bool {
        self.clock.is_synced()
    }

    /// Block until the clock has settled, or say it did not.
    ///
    /// Called once, on the node, before any plugin starts. This is the one
    /// place a node is allowed to wait: nothing downstream of it is running
    /// yet, so nothing can stall.
    pub fn wait_for_sync(&self, within: Duration) -> Result<()> {
        if self
            .clock
            .wait_for_sync(gst::ClockTime::from_nseconds(within.as_nanos() as u64))
            .is_ok()
        {
            tracing::info!(kind = self.kind.as_str(), "the node clock is synced to the core");
            return Ok(());
        }
        anyhow::bail!(
            "this node's {} clock did not sync to the core within {} seconds. Check that UDP \
             reaches the core's clock port and that no firewall is between them; the node will \
             not start a plugin until the clocks agree",
            self.kind.as_str(),
            within.as_secs()
        )
    }

    /// How far this node's idea of now sits from the machine's own clock, in
    /// milliseconds, and the spread of the last readings.
    ///
    /// Sampled here rather than asked of the core, because the whole point of
    /// the client clock is that it already knows. Twenty readings is about
    /// twenty seconds of heartbeats, which is long enough to see a drift and
    /// short enough to follow a correction.
    pub fn reading(&self) -> (f64, f64) {
        let system = gst::SystemClock::obtain();
        let (theirs, ours) = (self.clock.time(), system.time());
        let offset_ms =
            (theirs.nseconds() as f64 - ours.nseconds() as f64) / gst::ClockTime::MSECOND.nseconds() as f64;
        let mut samples = self.samples.lock();
        samples.push(offset_ms);
        if samples.len() > 20 {
            samples.remove(0);
        }
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let variance =
            samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / samples.len() as f64;
        (offset_ms, variance.sqrt())
    }
}

fn net_clock(host: &str, port: i32) -> Result<gst::Clock> {
    anyhow::ensure!(
        !host.is_empty(),
        "a node needs the core's address to follow its clock, and none was given"
    );
    let clock = gst_net::NetClientClock::new(Some("gmx-node-clock"), host, port, gst::ClockTime::ZERO);
    Ok(clock.upcast())
}

fn ptp_clock() -> Result<gst::Clock> {
    anyhow::ensure!(
        gst_net::PtpClock::is_supported(),
        "this GStreamer was built without PTP support"
    );
    gst_net::PtpClock::init(None, &[]).context(
        "start the PTP subsystem (it needs the gst-ptp-helper and a network card that will \
         timestamp)",
    )?;
    let clock =
        gst_net::PtpClock::new(Some("gmx-node-ptp"), 0).context("make a PTP clock for domain 0")?;
    Ok(clock.upcast())
}

/// Put a pipeline on a clock that is not its own, without letting it reset its
/// base time when it changes state.
///
/// The same two lines `Mixer::adopt_clock` does locally, on the node's side of
/// the wire. Keeping them together means the reason is written once.
pub fn adopt(pipeline: &gst::Pipeline, clock: &gst::Clock, base: Option<gst::ClockTime>) {
    pipeline.use_clock(Some(clock));
    pipeline.set_start_time(gst::ClockTime::NONE);
    if let Some(base) = base {
        pipeline.set_base_time(base);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_can_follow_the_core() {
        gst::init().unwrap();
        let core = gst::SystemClock::obtain();
        let provider = Provider::publish(&core, 0).unwrap();
        assert!(provider.port() > 0, "the kernel must have picked a port");

        let follower = Follower::follow("127.0.0.1", provider.port(), Kind::Net).unwrap();
        follower
            .wait_for_sync(Duration::from_secs(10))
            .expect("a client clock against a provider on this machine must sync");
        let (offset, jitter) = follower.reading();
        assert!(
            offset.abs() < 1_000.0,
            "two clocks on one machine must not be a second apart, got {offset} ms"
        );
        assert!(jitter >= 0.0);
    }

    #[test]
    fn a_clock_kind_round_trips() {
        for kind in [Kind::Net, Kind::Ptp] {
            assert_eq!(Kind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(Kind::parse(""), Some(Kind::Net));
        assert_eq!(Kind::parse("ntp"), None);
    }
}
