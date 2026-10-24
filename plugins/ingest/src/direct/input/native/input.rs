//! The native input itself: take the group's socket (shared), follow the
//! program, and hand tags on. Its thread is the one the host gives every
//! input; the socket thread is the group's.

use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use gmx_udp::recv::settings::Settings;
use serde_json::json;

use super::group::{Item, Key};
use super::program::Demux;
use super::stats::Report;
use super::{registry, timeline};
use crate::direct::input::outlet::Shared;
use crate::direct::input::spec::{Carry, InputError, InputSpec};
use crate::direct::{Input, Sink, StopSignal};
use crate::tagger::Zero;

/// A receive buffer of 16 MB unless `receive_buffer_kb` says otherwise:
/// three seconds of a 40 Mbit/s multiplex.
const BUFFER_KB: u64 = 16 * 1024;

pub struct Native {
    key: Key,
    address: String,
    buffer: usize,
    program: Option<u16>,
    carry: Carry,
}

impl Native {
    pub fn new(spec: &InputSpec) -> Result<Native, InputError> {
        let mut params = spec.params.clone();
        params["uri"] = json!(spec.uri);
        let bad = |e: String| InputError::new(e, json!({"field": "uri", "got": spec.uri}));
        let settings = Settings::from_params(&params).map_err(bad)?;
        let e = settings.endpoint().map_err(bad)?;
        let interface = settings.interface.clone();
        let address = gmx_udp::recv::elements::with_interface(&e.display(settings.scheme()), &interface);
        let kb = spec.number("receive_buffer_kb").unwrap_or(BUFFER_KB).clamp(64, 262_144);
        let key = Key { host: e.host, port: e.port, source: e.source, interface };
        Ok(Native { key, address, buffer: kb as usize * 1024, program: spec.program, carry: spec.carry })
    }
}

impl Input for Native {
    fn run(self: Box<Self>, sink: Sink, stop: StopSignal) {
        let out = Shared::new(sink);
        let sub = registry::subscribe(&self.key, &self.address, self.buffer, self.program);
        let mut demux = Demux::new(self.carry);
        let (zero, mut units) = (Zero::default(), Vec::new());
        let mut report = Report::new(self.program, &sub);
        let mut tick = Instant::now();
        while !stop.is_stopped() {
            match sub.rx.recv_timeout(Duration::from_millis(250)) {
                Ok(Item::Follow(number, pmt)) => demux.follow(number, pmt),
                Ok(Item::Packets(p)) => {
                    demux.feed(&p, &mut units);
                    for u in units.drain(..) {
                        out.tag(timeline::tag(u, &zero));
                    }
                }
                Ok(Item::Resumed) => demux.resumed(),
                Err(RecvTimeoutError::Timeout) => {}
                // The group's thread is gone, which only a failed start does.
                Err(RecvTimeoutError::Disconnected) => {
                    if stop.wait(Duration::from_millis(250)) {
                        break;
                    }
                }
            }
            if tick.elapsed() >= Duration::from_secs(1) {
                tick = Instant::now();
                report.publish(&out, &demux, &sub);
            }
        }
    }
}
