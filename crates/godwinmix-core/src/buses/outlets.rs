//! Where a bus goes: a sound card, an output's encoder, a monitoring tap.

use super::check::Refusal;
use super::desk::Desk;
use super::device::{self, DevicePlayer};
use super::encode::BusEncoder;
use super::DeviceFound;
use gstreamer as gst;

impl Desk {
    /// Play `bus` on `wanted`, or stop playing it for an empty one. Finding a
    /// named card can take seconds, so it runs on a thread of its own and the
    /// answer comes back through `notify`.
    pub(super) fn open_device(&mut self, bus: &str, wanted: &str) {
        self.tickets += 1;
        let ticket = self.tickets;
        let Some(live) = self.live.iter_mut().find(|l| l.graph.id == bus) else { return };
        if let Some(d) = live.device.take() {
            d.stop();
        }
        live.ticket = ticket;
        if wanted.is_empty() {
            live.device_state = None;
            return;
        }
        live.device_state = Some(format!("opening {wanted}"));
        let (bus, wanted) = (bus.to_string(), wanted.to_string());
        let Some(notify) = self.notify.clone() else {
            // A desk with nobody to answer to, as in a test: look here.
            let sink = device::sink_for(&wanted);
            self.device_found(DeviceFound { bus, ticket, sink });
            return;
        };
        let spawned = std::thread::Builder::new().name("bus-device".into()).spawn(move || {
            let sink = device::sink_for(&wanted);
            notify(DeviceFound { bus, ticket, sink });
        });
        if spawned.is_err() {
            tracing::warn!("no thread to look for a sound card on");
        }
    }

    pub(super) fn device_found(&mut self, found: DeviceFound) {
        let pipeline = self.pipeline.clone();
        let Some(live) = self.live.iter_mut().find(|l| l.graph.id == found.bus && l.ticket == found.ticket) else {
            return;
        };
        let started = found.sink.and_then(|sink| {
            DevicePlayer::start(&pipeline, &live.graph.tee, &found.bus, sink).map_err(|e| format!("{e:#}"))
        });
        match started {
            Ok(player) => {
                live.device = Some(player);
                live.device_state = None;
                tracing::info!(bus = %found.bus, "bus playing on a sound card");
            }
            Err(why) => {
                tracing::warn!(bus = %found.bus, %why, "a bus could not play on its sound card");
                live.device_state = Some(why);
            }
        }
    }

    /// The encoded sound of `bus`, for `output`, built if this is the first.
    pub fn encoder_for(&mut self, bus: &str, output: &str) -> Result<gst::Element, Refusal> {
        let pos = self.live.iter().position(|l| l.graph.id == bus).ok_or_else(|| self.no_bus(bus))?;
        let live = &mut self.live[pos];
        if live.encoder.is_none() {
            let enc = BusEncoder::start(&self.pipeline, &live.graph.tee, bus, &self.encode)
                .map_err(|e| Refusal::Failed(format!("bus {bus} could not be encoded for an output: {e:#}")))?;
            live.encoder = Some(enc);
        }
        let enc = live.encoder.as_mut().expect("just made");
        if !enc.users.iter().any(|u| u == output) {
            enc.users.push(output.to_string());
        }
        Ok(enc.tee.clone())
    }

    /// `output` no longer reads any bus. The last one out stops the encoder.
    pub fn release_output(&mut self, output: &str) {
        for live in &mut self.live {
            let Some(enc) = live.encoder.as_mut() else { continue };
            enc.users.retain(|u| u != output);
            if enc.users.is_empty() {
                if let Some(enc) = live.encoder.take() {
                    enc.stop();
                }
            }
        }
    }

    /// The raw sound of a bus, for a monitoring tap.
    pub fn tap_tee(&self, bus: &str) -> Result<gst::Element, Refusal> {
        Ok(self.live(bus)?.graph.tee.clone())
    }
}
